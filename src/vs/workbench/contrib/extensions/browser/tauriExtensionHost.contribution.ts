/*---------------------------------------------------------------------------------------------
 *  Tauri Extension Host Bridge for SideX
 *  Connects to the local Node.js extension host via WebSocket, syncs open
 *  documents, and wires language provider results back into Monaco.
 *--------------------------------------------------------------------------------------------*/

import { Disposable, IDisposable } from '../../../../base/common/lifecycle.js';
import { registerWorkbenchContribution2, WorkbenchPhase } from '../../../common/contributions.js';
import type { IWorkbenchContribution } from '../../../common/contributions.js';
import { ILogService } from '../../../../platform/log/common/log.js';
import { ILanguageFeaturesService } from '../../../../editor/common/services/languageFeatures.js';
import { IModelService } from '../../../../editor/common/services/model.js';
import { IMarkerService, MarkerSeverity } from '../../../../platform/markers/common/markers.js';
import type { IMarkerData } from '../../../../platform/markers/common/markers.js';
import { IBulkEditService, ResourceTextEdit } from '../../../../editor/browser/services/bulkEditService.js';
import { IEditorService } from '../../../services/editor/common/editorService.js';
import { IQuickInputService } from '../../../../platform/quickinput/common/quickInput.js';
import { ILanguageConfigurationService } from '../../../../editor/common/languages/languageConfigurationRegistry.js';
import type { LanguageConfiguration } from '../../../../editor/common/languages/languageConfiguration.js';
import type { ITextModel } from '../../../../editor/common/model.js';
import type { Position } from '../../../../editor/common/core/position.js';
import type { CancellationToken } from '../../../../base/common/cancellation.js';
import type {
	CompletionContext,
	CompletionItem,
	CompletionList,
	Hover,
	Location,
	DocumentSymbol,
	CodeActionList,
	CodeLensList,
	TextEdit,
	SignatureHelpResult,
	DocumentHighlight,
	WorkspaceEdit,
	Rejection,
	RenameLocation,
	FoldingRange,
	InlayHintList,
	IWorkspaceTextEdit,
	ILinksList,
	IColorInformation,
	IColorPresentation,
	SelectionRange,
	SemanticTokens,
	SemanticTokensLegend,
} from '../../../../editor/common/languages.js';
import {
	CompletionItemKind,
	DocumentHighlightKind,
	SymbolKind,
} from '../../../../editor/common/languages.js';
import type { LanguageSelector } from '../../../../editor/common/languageSelector.js';
import { URI } from '../../../../base/common/uri.js';
import { Range } from '../../../../editor/common/core/range.js';

// ── Helpers ──────────────────────────────────────────────────────────────────

function modelToParams(model: ITextModel, position: Position) {
	return {
		uri: model.uri.toString(),
		languageId: model.getLanguageId(),
		version: model.getVersionId(),
		position: { line: position.lineNumber - 1, character: position.column - 1 },
	};
}

function toVscPosition(pos: { line: number; character: number }): { lineNumber: number; column: number } {
	return { lineNumber: pos.line + 1, column: pos.character + 1 };
}

function toVscRange(r: { start: { line: number; character: number }; end: { line: number; character: number } }) {
	return {
		startLineNumber: r.start.line + 1,
		startColumn: r.start.character + 1,
		endLineNumber: r.end.line + 1,
		endColumn: r.end.character + 1,
	};
}

// ── Extension Host Contribution ───────────────────────────────────────────────

interface HandshakeMessage {
	type: 'sidex:handshake';
	connectionToken: string;
	reconnectionToken: string;
	extensionCount: number;
	extensions: { id: string; name: string }[];
}

type ProviderCapabilities = Record<string, unknown[][]>;

class TauriExtensionHostContribution extends Disposable implements IWorkbenchContribution {

	static readonly ID = 'workbench.contrib.tauriExtensionHost';

	private _ws: WebSocket | undefined;
	private _port: number | undefined;
	private _msgId = 0;
	private _reconnectTimer: ReturnType<typeof setTimeout> | undefined;
	private _reconnectAttempts = 0;
	private _pendingCallbacks = new Map<number, { resolve: (v: unknown) => void; reject: (e: Error) => void }>();
	private _connected = false;
	private _providerRegistrations: IDisposable[] = [];

	constructor(
		@ILogService private readonly logService: ILogService,
		@ILanguageFeaturesService private readonly languageFeatures: ILanguageFeaturesService,
		@IModelService private readonly modelService: IModelService,
		@IMarkerService private readonly markerService: IMarkerService,
		@IBulkEditService private readonly bulkEditService: IBulkEditService,
		@IEditorService private readonly editorService: IEditorService,
		@IQuickInputService private readonly quickInputService: IQuickInputService,
		@ILanguageConfigurationService private readonly langConfigService: ILanguageConfigurationService,
	) {
		super();
		this._init();
	}

	// ── Lifecycle ───────────────────────────────────────────────────────────

	private async _init(): Promise<void> {
		if ((globalThis as any).__SIDEX_TAURI__ !== true) {
			return;
		}
		try {
			const { invoke } = await import('@tauri-apps/api/core');
			const port = await invoke<number>('start_extension_host');
			this._port = port;
			this._connect(port);
		} catch {
			// Extension host is optional — silently skip if unavailable
		}
	}

	private _connect(port: number): void {
		try {
			const ws = new WebSocket(`ws://127.0.0.1:${port}`);
			this._ws = ws;

			ws.onopen = () => {
				this._connected = true;
				this._reconnectAttempts = 0;
				this._send({ id: this._nextId(), type: 'initialize', params: { extensionPaths: [], workspaceFolders: [] } });
				this._syncOpenDocuments();
				this._syncActiveEditor();
			};

			ws.onmessage = (event) => {
				try {
					this._handleMessage(JSON.parse(event.data as string));
				} catch { /* ignore malformed messages */ }
			};

			ws.onerror = () => { /* handled by onclose */ };

			ws.onclose = () => {
				this._ws = undefined;
				this._connected = false;
				this._scheduleReconnect();
			};

			window.addEventListener('beforeunload', () => {
				if (this._ws?.readyState === WebSocket.OPEN) {
					this._ws.close(1000, 'page-unload');
					this._ws = undefined;
				}
			}, { once: true });
		} catch {
			this._scheduleReconnect();
		}
	}

	private _scheduleReconnect(): void {
		if (this._reconnectTimer || !this._port || this._reconnectAttempts >= 3) {
			return;
		}
		this._reconnectAttempts++;
		const delay = Math.min(1000 * Math.pow(2, this._reconnectAttempts - 1), 30000);
		this._reconnectTimer = setTimeout(() => {
			this._reconnectTimer = undefined;
			if (this._port && (!this._ws || this._ws.readyState === WebSocket.CLOSED)) {
				this._connect(this._port);
			}
		}, delay);
	}

	override dispose(): void {
		clearTimeout(this._reconnectTimer);
		this._ws?.close();
		this._ws = undefined;
		for (const [, cb] of this._pendingCallbacks) {
			cb.reject(new Error('disposed'));
		}
		this._pendingCallbacks.clear();
		this._providerRegistrations.forEach(d => d.dispose());
		this._langConfigDisposables.forEach(d => d.dispose());
		this._tauriWatchUnlisten?.();
		for (const [watcherId] of this._activeWatches) {
			this._onStopFileWatch(watcherId);
		}
		super.dispose();
	}

	// ── Messaging ────────────────────────────────────────────────────────────

	private _nextId(): number {
		return ++this._msgId;
	}

	private _send(msg: Record<string, unknown>): void {
		if (this._ws?.readyState === WebSocket.OPEN) {
			this._ws.send(JSON.stringify(msg));
		}
	}

	private _request<T = unknown>(type: string, params?: Record<string, unknown>): Promise<T> {
		return new Promise((resolve, reject) => {
			const id = this._nextId();
			this._pendingCallbacks.set(id, { resolve: resolve as (v: unknown) => void, reject });
			this._send({ id, type, params });
			setTimeout(() => {
				if (this._pendingCallbacks.delete(id)) {
					reject(new Error(`ExtHost request '${type}' timed out`));
				}
			}, 10000);
		});
	}

	private _extensionCount = 0;
	private _activatedCount = 0;
	private _capabilitiesQueried = false;

	private _handleMessage(msg: any): void {
		if (msg.id !== undefined && this._pendingCallbacks.has(msg.id)) {
			const cb = this._pendingCallbacks.get(msg.id)!;
			this._pendingCallbacks.delete(msg.id);
			msg.error
				? cb.reject(new Error(String(msg.error)))
				: cb.resolve(msg.result);
			return;
		}

		switch (msg.type) {
			case 'sidex:handshake':
				this._onHandshake(msg as HandshakeMessage);
				break;
			case 'extensionActivated':
				this._activatedCount++;
				if (!this._capabilitiesQueried && this._activatedCount >= this._extensionCount) {
					this._capabilitiesQueried = true;
					this._queryAndRegisterProviders();
				}
				break;
			case 'diagnosticsChanged':
				this._onDiagnosticsChanged(msg.uri, msg.diagnostics);
				break;
			case 'applyEdit':
				this._onApplyEdit(msg.edits);
				break;
			case 'showMessage':
				this.logService.info(`[ExtHost] ${msg.severity}: ${msg.message}`);
				break;
			case 'showTextDocument':
				this._onShowTextDocument(msg.uri, msg.options);
				break;
			case 'showQuickPick':
				this._onShowQuickPick(msg.id, msg.items, msg.options);
				break;
			case 'showInputBox':
				this._onShowInputBox(msg.id, msg.options);
				break;
			case 'showMessageRequest':
				this._onShowMessageRequest(msg.id, msg.severity, msg.message, msg.items);
				break;
			case 'languageConfigurationChanged':
				this._onLanguageConfigurationChanged(msg.language, msg.configuration);
				break;
			case 'startFileWatch':
				this._onStartFileWatch(msg.watcherId, msg.paths, msg.pattern, msg.recursive);
				break;
			case 'stopFileWatch':
				this._onStopFileWatch(msg.watcherId);
				break;
		}
	}

	private _onHandshake(msg: HandshakeMessage): void {
		this._extensionCount = msg.extensionCount;
		this._activatedCount = 0;
		this._capabilitiesQueried = false;
		this.logService.info(`[ExtHost] Connected — ${msg.extensionCount} extensions`);

		// Activate each discovered extension
		for (const ext of msg.extensions) {
			this._send({ id: this._nextId(), type: 'activateExtension', params: { extensionId: ext.id } });
		}

		// Fallback: query capabilities after 3s in case activate events are missed
		setTimeout(() => {
			if (!this._capabilitiesQueried) {
				this._capabilitiesQueried = true;
				this._queryAndRegisterProviders();
			}
		}, 3000);
	}

	private async _queryAndRegisterProviders(): Promise<void> {
		try {
			const caps = await this._request<ProviderCapabilities>('getProviderCapabilities');
			this._registerProviders(caps);
		} catch (e) {
			this.logService.warn('[ExtHost] Could not get provider capabilities:', e);
		}
	}

	private _registerProviders(caps: ProviderCapabilities): void {
		// Dispose any previous registrations (e.g. after reconnect)
		this._providerRegistrations.forEach(d => d.dispose());
		this._providerRegistrations = [];

		const selectors = (selectorList: unknown[][]): LanguageSelector => {
			const all: string[] = [];
			for (const s of selectorList.flat()) {
				if (typeof s === 'string') {
					all.push(s);
				} else if (s && typeof (s as any).language === 'string') {
					all.push((s as any).language);
				}
			}
			const unique = [...new Set(all)];
			return unique.length > 0 ? unique as LanguageSelector : '*';
		};

		if (caps.completion) {
			this._providerRegistrations.push(
				this.languageFeatures.completionProvider.register(selectors(caps.completion), {
					_debugDisplayName: 'tauriExtHost',
					provideCompletionItems: (model, position, context, _token) =>
						this._provideCompletionItems(model, position, context),
				})
			);
		}

		if (caps.hover) {
			this._providerRegistrations.push(
				this.languageFeatures.hoverProvider.register(selectors(caps.hover), {
					provideHover: (model, position, _token) =>
						this._provideHover(model, position),
				})
			);
		}

		if (caps.definition) {
			this._providerRegistrations.push(
				this.languageFeatures.definitionProvider.register(selectors(caps.definition), {
					provideDefinition: (model, position, _token) =>
						this._provideDefinition(model, position),
				})
			);
		}

		if (caps.typeDefinition) {
			this._providerRegistrations.push(
				this.languageFeatures.typeDefinitionProvider.register(selectors(caps.typeDefinition), {
					provideTypeDefinition: (model, position, _token) =>
						this._provideGenericLocations('provideTypeDefinition', model, position),
				})
			);
		}

		if (caps.implementation) {
			this._providerRegistrations.push(
				this.languageFeatures.implementationProvider.register(selectors(caps.implementation), {
					provideImplementation: (model, position, _token) =>
						this._provideGenericLocations('provideImplementation', model, position),
				})
			);
		}

		if (caps.declaration) {
			this._providerRegistrations.push(
				this.languageFeatures.declarationProvider.register(selectors(caps.declaration), {
					provideDeclaration: (model, position, _token) =>
						this._provideGenericLocations('provideDeclaration', model, position),
				})
			);
		}

		if (caps.references) {
			this._providerRegistrations.push(
				this.languageFeatures.referenceProvider.register(selectors(caps.references), {
					provideReferences: (model, position, _context, _token) =>
						this._provideReferences(model, position),
				})
			);
		}

		if (caps.documentSymbol) {
			this._providerRegistrations.push(
				this.languageFeatures.documentSymbolProvider.register(selectors(caps.documentSymbol), {
					provideDocumentSymbols: (model, _token) =>
						this._provideDocumentSymbols(model),
				})
			);
		}

		if (caps.codeAction) {
			this._providerRegistrations.push(
				this.languageFeatures.codeActionProvider.register(selectors(caps.codeAction), {
					provideCodeActions: (model, rangeOrSelection, context, _token) =>
						this._provideCodeActions(model, rangeOrSelection, context),
				})
			);
		}

		if (caps.codeLens) {
			this._providerRegistrations.push(
				this.languageFeatures.codeLensProvider.register(selectors(caps.codeLens), {
					provideCodeLenses: (model, _token) =>
						this._provideCodeLenses(model),
				})
			);
		}

		if (caps.formatting) {
			this._providerRegistrations.push(
				this.languageFeatures.documentFormattingEditProvider.register(selectors(caps.formatting), {
					provideDocumentFormattingEdits: (model, options, _token) =>
						this._provideFormatting(model, options),
				})
			);
		}

		if (caps.rangeFormatting) {
			this._providerRegistrations.push(
				this.languageFeatures.documentRangeFormattingEditProvider.register(selectors(caps.rangeFormatting), {
					provideDocumentRangeFormattingEdits: (model, range, options, _token) =>
						this._provideRangeFormatting(model, range, options),
				})
			);
		}

		if (caps.signatureHelp) {
			this._providerRegistrations.push(
				this.languageFeatures.signatureHelpProvider.register(selectors(caps.signatureHelp), {
					signatureHelpTriggerCharacters: ['(', ','],
					signatureHelpRetriggerCharacters: [','],
					provideSignatureHelp: (model, position, _token, context) =>
						this._provideSignatureHelp(model, position, context),
				})
			);
		}

		if (caps.documentHighlight) {
			this._providerRegistrations.push(
				this.languageFeatures.documentHighlightProvider.register(selectors(caps.documentHighlight), {
					provideDocumentHighlights: (model, position, _token) =>
						this._provideDocumentHighlights(model, position),
				})
			);
		}

		if (caps.rename) {
			this._providerRegistrations.push(
				this.languageFeatures.renameProvider.register(selectors(caps.rename), {
					provideRenameEdits: (model, position, newName, _token) =>
						this._provideRenameEdits(model, position, newName),
					resolveRenameLocation: (model, position, _token) =>
						this._resolveRenameLocation(model, position),
				})
			);
		}

		if (caps.documentLink) {
			this._providerRegistrations.push(
				this.languageFeatures.linkProvider.register(selectors(caps.documentLink), {
					provideLinks: (model, _token) =>
						this._provideDocumentLinks(model),
				})
			);
		}

		if (caps.foldingRange) {
			this._providerRegistrations.push(
				this.languageFeatures.foldingRangeProvider.register(selectors(caps.foldingRange), {
					provideFoldingRanges: (model, _context, _token) =>
						this._provideFoldingRanges(model),
				})
			);
		}

		if (caps.inlayHint) {
			this._providerRegistrations.push(
				this.languageFeatures.inlayHintsProvider.register(selectors(caps.inlayHint), {
					provideInlayHints: (model, range, _token) =>
						this._provideInlayHints(model, range),
				})
			);
		}

		if (caps.selectionRange) {
			this._providerRegistrations.push(
				this.languageFeatures.selectionRangeProvider.register(selectors(caps.selectionRange), {
					provideSelectionRanges: (model, positions, _token) =>
						this._provideSelectionRanges(model, positions),
				})
			);
		}

		if (caps.semanticTokens) {
			this._providerRegistrations.push(
				this.languageFeatures.documentSemanticTokensProvider.register(selectors(caps.semanticTokens), {
					getLegend: () => this._semanticTokensLegend,
					provideDocumentSemanticTokens: (model, lastResultId, _token) =>
						this._provideSemanticTokens(model),
					releaseDocumentSemanticTokens: () => {},
				})
			);
		}

		if (caps.color) {
			this._providerRegistrations.push(
				this.languageFeatures.colorProvider.register(selectors(caps.color), {
					provideDocumentColors: (model, _token) =>
						this._provideDocumentColors(model),
					provideColorPresentations: (_model, _colorInfo, _token) =>
						Promise.resolve([]),
				})
			);
		}

		this.logService.info(`[ExtHost] Registered providers for: ${Object.keys(caps).join(', ')}`);
	}

	// ── Language Providers ────────────────────────────────────────────────────

	private async _provideCompletionItems(
		model: ITextModel,
		position: Position,
		context: CompletionContext,
	): Promise<CompletionList | null> {
		try {
			const result = await this._request<{ items: any[] } | null>('provideCompletionItems', {
				...modelToParams(model, position),
				triggerCharacter: context.triggerCharacter,
			});
			if (!result?.items?.length) {
				return null;
			}
			return {
				suggestions: result.items.map(item => this._convertCompletionItem(item)),
				incomplete: false,
			};
		} catch {
			return null;
		}
	}

	private async _provideHover(model: ITextModel, position: Position): Promise<Hover | null> {
		try {
			const result = await this._request<{ contents: any[]; range?: any } | null>('provideHover', modelToParams(model, position));
			if (!result?.contents?.length) {
				return null;
			}
			return {
				contents: result.contents.map(c => ({ value: typeof c === 'string' ? c : String(c?.value ?? '') })),
				range: result.range ? toVscRange(result.range) : undefined,
			};
		} catch {
			return null;
		}
	}

	private async _provideDefinition(model: ITextModel, position: Position): Promise<Location | Location[] | null> {
		try {
			const result = await this._request<any>('provideDefinition', modelToParams(model, position));
			return result ? this._convertLocations(result) : null;
		} catch {
			return null;
		}
	}

	private async _provideGenericLocations(method: string, model: ITextModel, position: Position): Promise<Location | Location[] | null> {
		try {
			const result = await this._request<any>(method, modelToParams(model, position));
			return result ? this._convertLocations(result) : null;
		} catch {
			return null;
		}
	}

	private async _provideReferences(model: ITextModel, position: Position): Promise<Location[]> {
		try {
			const result = await this._request<any[]>('provideReferences', modelToParams(model, position));
			return Array.isArray(result) ? result.map(l => this._convertLocation(l)) : [];
		} catch {
			return [];
		}
	}

	private async _provideDocumentSymbols(model: ITextModel): Promise<DocumentSymbol[] | null> {
		try {
			const result = await this._request<any[]>('provideDocumentSymbols', {
				uri: model.uri.toString(),
				languageId: model.getLanguageId(),
				version: model.getVersionId(),
			});
			if (!Array.isArray(result) || !result.length) {
				return null;
			}
			return result.map(s => this._convertDocumentSymbol(s));
		} catch {
			return null;
		}
	}

	private async _provideCodeActions(
		model: ITextModel,
		rangeOrSelection: any,
		context: any,
	): Promise<CodeActionList | null> {
		try {
			const range = {
				start: { line: rangeOrSelection.startLineNumber - 1, character: rangeOrSelection.startColumn - 1 },
				end: { line: rangeOrSelection.endLineNumber - 1, character: rangeOrSelection.endColumn - 1 },
			};
			const result = await this._request<any[]>('provideCodeActions', {
				uri: model.uri.toString(),
				languageId: model.getLanguageId(),
				version: model.getVersionId(),
				range,
				context: { diagnostics: context.markers || [], triggerKind: context.trigger, only: context.only?.value },
			});
			if (!Array.isArray(result) || !result.length) {
				return null;
			}
			return {
				actions: result.map(a => ({
					title: a.title,
					kind: a.kind,
					diagnostics: a.diagnostics || [],
					isPreferred: a.isPreferred || false,
					edit: a.edit ? this._convertWorkspaceEdit(a.edit) : undefined,
					command: a.command,
				})),
				dispose: () => {},
			} as CodeActionList;
		} catch {
			return null;
		}
	}

	private async _provideCodeLenses(model: ITextModel): Promise<CodeLensList | null> {
		try {
			const result = await this._request<any[]>('provideCodeLenses', {
				uri: model.uri.toString(),
				languageId: model.getLanguageId(),
				version: model.getVersionId(),
			});
			if (!Array.isArray(result) || !result.length) {
				return null;
			}
			return {
				lenses: result.map(l => ({
					range: toVscRange(l.range),
					command: l.command ? { id: l.command.command || l.command.id, title: l.command.title, arguments: l.command.arguments } : undefined,
				})),
				dispose: () => {},
			};
		} catch {
			return null;
		}
	}

	private async _provideFormatting(model: ITextModel, options: any): Promise<TextEdit[] | null> {
		try {
			const result = await this._request<any[]>('provideFormatting', {
				uri: model.uri.toString(),
				languageId: model.getLanguageId(),
				version: model.getVersionId(),
				options,
			});
			if (!Array.isArray(result) || !result.length) {
				return null;
			}
			return result.map(e => ({ range: toVscRange(e.range), text: e.newText }));
		} catch {
			return null;
		}
	}

	private async _provideRangeFormatting(model: ITextModel, range: any, options: any): Promise<TextEdit[] | null> {
		try {
			const r = {
				start: { line: range.startLineNumber - 1, character: range.startColumn - 1 },
				end: { line: range.endLineNumber - 1, character: range.endColumn - 1 },
			};
			const result = await this._request<any[]>('provideRangeFormatting', {
				uri: model.uri.toString(),
				languageId: model.getLanguageId(),
				version: model.getVersionId(),
				range: r,
				options,
			});
			if (!Array.isArray(result) || !result.length) {
				return null;
			}
			return result.map(e => ({ range: toVscRange(e.range), text: e.newText }));
		} catch {
			return null;
		}
	}

	private async _provideSignatureHelp(model: ITextModel, position: Position, context: any): Promise<SignatureHelpResult | null> {
		try {
			const result = await this._request<any>('provideSignatureHelp', {
				...modelToParams(model, position),
				context: { triggerKind: context.triggerKind, triggerCharacter: context.triggerCharacter, isRetrigger: context.isRetrigger },
			});
			if (!result?.signatures?.length) {
				return null;
			}
			return {
				value: {
					signatures: result.signatures.map((s: any) => ({
						label: s.label,
						documentation: s.documentation ? { value: typeof s.documentation === 'string' ? s.documentation : s.documentation.value || '' } : undefined,
						parameters: (s.parameters || []).map((p: any) => ({
							label: p.label,
							documentation: p.documentation ? { value: typeof p.documentation === 'string' ? p.documentation : p.documentation.value || '' } : undefined,
						})),
					})),
					activeSignature: result.activeSignature ?? 0,
					activeParameter: result.activeParameter ?? 0,
				},
				dispose: () => {},
			};
		} catch {
			return null;
		}
	}

	private async _provideDocumentHighlights(model: ITextModel, position: Position): Promise<DocumentHighlight[] | null> {
		try {
			const result = await this._request<any[]>('provideDocumentHighlight', modelToParams(model, position));
			if (!Array.isArray(result) || !result.length) {
				return null;
			}
			return result.map(h => ({
				range: toVscRange(h.range),
				kind: h.kind ?? DocumentHighlightKind.Text,
			}));
		} catch {
			return null;
		}
	}

	private async _provideRenameEdits(model: ITextModel, position: Position, newName: string): Promise<WorkspaceEdit & Rejection | null> {
		try {
			const result = await this._request<any>('provideRename', {
				...modelToParams(model, position),
				newName,
			});
			if (!result?.edits?.length) {
				return null;
			}
			return this._convertWorkspaceEdit(result);
		} catch {
			return null;
		}
	}

	private async _resolveRenameLocation(model: ITextModel, position: Position): Promise<RenameLocation | null> {
		try {
			const result = await this._request<any>('prepareRename', modelToParams(model, position));
			if (!result?.range) {
				return null;
			}
			return {
				range: toVscRange(result.range),
				text: result.placeholder || model.getWordAtPosition(position)?.word || '',
			};
		} catch {
			return null;
		}
	}

	private async _provideDocumentLinks(model: ITextModel): Promise<ILinksList | null> {
		try {
			const result = await this._request<any[]>('provideDocumentLinks', {
				uri: model.uri.toString(),
				languageId: model.getLanguageId(),
				version: model.getVersionId(),
			});
			if (!Array.isArray(result) || !result.length) {
				return null;
			}
			return {
				links: result.map(l => ({
					range: toVscRange(l.range),
					url: l.target ? URI.parse(l.target) : undefined,
				})),
			};
		} catch {
			return null;
		}
	}

	private async _provideFoldingRanges(model: ITextModel): Promise<FoldingRange[] | null> {
		try {
			const result = await this._request<any[]>('provideFoldingRanges', {
				uri: model.uri.toString(),
				languageId: model.getLanguageId(),
				version: model.getVersionId(),
			});
			if (!Array.isArray(result) || !result.length) {
				return null;
			}
			return result.map(f => ({
				start: f.start + 1,
				end: f.end + 1,
				kind: f.kind !== undefined ? { value: f.kind } : undefined,
			}));
		} catch {
			return null;
		}
	}

	private async _provideInlayHints(model: ITextModel, range: any): Promise<InlayHintList | null> {
		try {
			const r = {
				start: { line: range.startLineNumber - 1, character: range.startColumn - 1 },
				end: { line: range.endLineNumber - 1, character: range.endColumn - 1 },
			};
			const result = await this._request<any[]>('provideInlayHints', {
				uri: model.uri.toString(),
				languageId: model.getLanguageId(),
				version: model.getVersionId(),
				range: r,
			});
			if (!Array.isArray(result) || !result.length) {
				return null;
			}
			return {
				hints: result.map(h => ({
					label: typeof h.label === 'string' ? h.label : (Array.isArray(h.label) ? h.label.map((p: any) => ({ label: p.value || '' })) : String(h.label)),
					position: toVscPosition(h.position),
					kind: h.kind,
					paddingLeft: h.paddingLeft,
					paddingRight: h.paddingRight,
				})),
				dispose: () => {},
			};
		} catch {
			return null;
		}
	}

	// ── Document Synchronisation ──────────────────────────────────────────────

	private _syncOpenDocuments(): void {
		// Notify extension host about all currently open models
		for (const model of this.modelService.getModels()) {
			if (model.uri.scheme === 'file') {
				this._notifyDocumentOpened(model);
			}
		}

		// Listen for new models
		this._register(this.modelService.onModelAdded(model => {
			if (model.uri.scheme === 'file') {
				this._notifyDocumentOpened(model);
			}
		}));

		// Listen for model changes
		this._register(this.modelService.onModelRemoved(model => {
			if (model.uri.scheme === 'file') {
				this._send({ id: this._nextId(), type: 'documentClosed', params: { uri: model.uri.toString() } });
			}
		}));
	}

	private _notifyDocumentOpened(model: ITextModel): void {
		this._send({
			id: this._nextId(),
			type: 'documentOpened',
			params: {
				uri: model.uri.toString(),
				languageId: model.getLanguageId(),
				version: model.getVersionId(),
				text: model.getValue(),
			},
		});

		// Track changes after open
		this._register(model.onDidChangeContent(e => {
			if (this._connected) {
				const changes = e.changes.map(c => ({
					range: {
						start: { line: c.range.startLineNumber - 1, character: c.range.startColumn - 1 },
						end: { line: c.range.endLineNumber - 1, character: c.range.endColumn - 1 },
					},
					rangeOffset: c.rangeOffset,
					rangeLength: c.rangeLength,
					text: c.text,
				}));
				this._send({
					id: this._nextId(),
					type: 'documentChanged',
					params: {
						uri: model.uri.toString(),
						version: model.getVersionId(),
						text: model.getValue(),
						changes,
					},
				});
			}
		}));
	}

	// ── Diagnostics ───────────────────────────────────────────────────────────

	private static readonly _DIAG_OWNER = 'tauriExtHost';

	private _onDiagnosticsChanged(uri: string, diagnostics: any[]): void {
		if (!uri) {
			return;
		}
		const resource = URI.parse(uri);
		const markers: IMarkerData[] = (diagnostics || []).map(d => {
			const sev = d.severity === 0 ? MarkerSeverity.Error
				: d.severity === 1 ? MarkerSeverity.Warning
				: d.severity === 2 ? MarkerSeverity.Info
				: MarkerSeverity.Hint;
			return {
				severity: sev,
				message: d.message || '',
				source: d.source || '',
				code: d.code || undefined,
				startLineNumber: (d.range?.start?.line ?? 0) + 1,
				startColumn: (d.range?.start?.character ?? 0) + 1,
				endLineNumber: (d.range?.end?.line ?? 0) + 1,
				endColumn: (d.range?.end?.character ?? 0) + 1,
			};
		});
		this.markerService.changeOne(TauriExtensionHostContribution._DIAG_OWNER, resource, markers);
	}

	// ── Workspace Apply Edit ──────────────────────────────────────────────────

	private async _onApplyEdit(edits: any[]): Promise<void> {
		if (!edits?.length) {
			return;
		}
		try {
			const textEdits = edits.map(e => new ResourceTextEdit(
				URI.parse(e.uri),
				{
					range: toVscRange(e.range),
					text: e.newText,
				},
			));
			await this.bulkEditService.apply(textEdits);
		} catch (e) {
			this.logService.warn('[ExtHost] applyEdit failed:', e);
		}
	}

	// ── Show Text Document ────────────────────────────────────────────────────

	private async _onShowTextDocument(uri: string, options?: any): Promise<void> {
		if (!uri) {
			return;
		}
		try {
			await this.editorService.openEditor({ resource: URI.parse(uri) });
		} catch {
			// best-effort
		}
	}

	// ── Active Editor Sync ────────────────────────────────────────────────────

	private _syncActiveEditor(): void {
		const sendActive = () => {
			const editor = this.editorService.activeTextEditorControl;
			const model = editor && 'getModel' in editor ? (editor as any).getModel() as ITextModel | null : null;
			if (model && model.uri.scheme === 'file') {
				this._send({
					id: this._nextId(),
					type: 'activeEditorChanged',
					params: {
						uri: model.uri.toString(),
						languageId: model.getLanguageId(),
					},
				});
			} else {
				this._send({ id: this._nextId(), type: 'activeEditorChanged', params: { uri: null } });
			}
		};
		sendActive();
		this._register(this.editorService.onDidActiveEditorChange(() => {
			if (this._connected) {
				sendActive();
			}
		}));
	}

	// ── Quick Pick / Input Box / Message Request ──────────────────────────────

	private async _onShowQuickPick(requestId: number, items: string[], options: any): Promise<void> {
		try {
			const pickItems = items.map(label => ({ label }));
			const picked = await this.quickInputService.pick(pickItems, { placeHolder: options?.placeHolder });
			const value = picked ? (picked as { label: string }).label : undefined;
			this._send({ id: this._nextId(), type: 'messageResponse', params: { requestId, value } });
		} catch {
			this._send({ id: this._nextId(), type: 'messageResponse', params: { requestId, value: undefined } });
		}
	}

	private async _onShowInputBox(requestId: number, options: any): Promise<void> {
		try {
			const value = await this.quickInputService.input({
				placeHolder: options?.placeHolder,
				prompt: options?.prompt,
				value: options?.value,
				password: options?.password,
			});
			this._send({ id: this._nextId(), type: 'messageResponse', params: { requestId, value } });
		} catch {
			this._send({ id: this._nextId(), type: 'messageResponse', params: { requestId, value: undefined } });
		}
	}

	private async _onShowMessageRequest(requestId: number, severity: string, message: string, items: string[]): Promise<void> {
		// Use the quick input service to show choices
		if (!items?.length) {
			return;
		}
		try {
			const picked = await this.quickInputService.pick(
				items.map(label => ({ label })),
				{ placeHolder: message },
			);
			const value = picked ? (Array.isArray(picked) ? picked[0]?.label : picked.label) : undefined;
			this._send({ id: this._nextId(), type: 'messageResponse', params: { requestId, value } });
		} catch {
			this._send({ id: this._nextId(), type: 'messageResponse', params: { requestId, value: undefined } });
		}
	}

	// ── Language Configuration ────────────────────────────────────────────────

	private _langConfigDisposables = new Map<string, IDisposable>();

	private _onLanguageConfigurationChanged(language: string, config: any): void {
		if (!language || !config) {
			return;
		}
		// Dispose previous registration for this language
		this._langConfigDisposables.get(language)?.dispose();

		const langConfig: LanguageConfiguration = {};
		if (config.comments) {
			langConfig.comments = {
				lineComment: config.comments.lineComment ?? null,
				blockComment: config.comments.blockComment ?? null,
			};
		}
		if (config.brackets) {
			langConfig.brackets = config.brackets;
		}
		if (config.autoClosingPairs) {
			langConfig.autoClosingPairs = config.autoClosingPairs;
		}
		if (config.surroundingPairs) {
			langConfig.surroundingPairs = config.surroundingPairs;
		}
		if (config.wordPattern) {
			try { langConfig.wordPattern = new RegExp(config.wordPattern); } catch { /* ignore invalid regex */ }
		}
		if (config.indentationRules) {
			try {
				langConfig.indentationRules = {
					increaseIndentPattern: config.indentationRules.increaseIndentPattern ? new RegExp(config.indentationRules.increaseIndentPattern) : /(?:)/,
					decreaseIndentPattern: config.indentationRules.decreaseIndentPattern ? new RegExp(config.indentationRules.decreaseIndentPattern) : /(?:)/,
				};
			} catch { /* ignore invalid regex */ }
		}

		const disposable = this.langConfigService.register(language, langConfig);
		this._langConfigDisposables.set(language, disposable);
	}

	// ── File System Watcher ───────────────────────────────────────────────────

	private _activeWatches = new Map<number, number>(); // watcherId → Tauri watch_id
	private _tauriWatchUnlisten: (() => void) | undefined;

	private async _onStartFileWatch(watcherId: number, paths: string[], pattern: string, recursive: boolean): Promise<void> {
		try {
			const { invoke } = await import('@tauri-apps/api/core');

			// Extract file extensions from the glob pattern for Tauri's watch_start
			let fileExtensions: string[] | undefined;
			const extMatch = pattern.match(/\*\.(\w+)$/);
			if (extMatch) {
				fileExtensions = [extMatch[1]];
			}

			const tauriWatchId = await invoke<number>('watch_start', {
				paths,
				options: {
					recursive: recursive !== false,
					debounce_ms: 200,
					file_extensions: fileExtensions,
					ignore_patterns: ['node_modules', '.git', '*.log'],
					emit_content: false,
				},
			});
			this._activeWatches.set(watcherId, tauriWatchId);
			this.logService.info(`[ExtHost] File watch ${watcherId} started (tauri=${tauriWatchId}) for ${pattern}`);

			// Start listening for Tauri watch events if not already
			if (!this._tauriWatchUnlisten) {
				this._setupTauriWatchListener();
			}
		} catch (e) {
			this.logService.warn(`[ExtHost] Failed to start file watch for ${pattern}:`, e);
		}
	}

	private async _onStopFileWatch(watcherId: number): Promise<void> {
		const tauriWatchId = this._activeWatches.get(watcherId);
		if (tauriWatchId === undefined) {
			return;
		}
		this._activeWatches.delete(watcherId);
		try {
			const { invoke } = await import('@tauri-apps/api/core');
			await invoke('watch_stop', { id: tauriWatchId });
			this.logService.info(`[ExtHost] File watch ${watcherId} stopped`);
		} catch {
			// best-effort cleanup
		}
	}

	private async _setupTauriWatchListener(): Promise<void> {
		try {
			const { listen } = await import('@tauri-apps/api/event');
			const unlisten = await listen<{ watch_id: number; events: { path: string; kind: string; is_dir: boolean }[] }>('watch-batch', (event) => {
				if (!this._connected) {
					return;
				}
				this._send({
					id: this._nextId(),
					type: 'fileWatchEvent',
					params: { events: event.payload.events },
				});
			});
			this._tauriWatchUnlisten = unlisten;
		} catch (e) {
			this.logService.warn('[ExtHost] Failed to setup Tauri watch listener:', e);
		}
	}

	// ── Additional Language Providers ──────────────────────────────────────────

	private async _provideSelectionRanges(model: ITextModel, positions: Position[]): Promise<SelectionRange[][] | null> {
		try {
			const result = await this._request<any[][]>('provideSelectionRanges', {
				uri: model.uri.toString(),
				languageId: model.getLanguageId(),
				version: model.getVersionId(),
				positions: positions.map(p => ({ line: p.lineNumber - 1, character: p.column - 1 })),
			});
			if (!Array.isArray(result)) {
				return null;
			}
			return result.map(positionRanges => {
				const ranges: SelectionRange[] = [];
				let current: any = Array.isArray(positionRanges) ? positionRanges[0] : positionRanges;
				while (current) {
					ranges.push({ range: toVscRange(current.range || current) });
					current = current.parent;
				}
				return ranges;
			});
		} catch {
			return null;
		}
	}

	private _semanticTokensLegend: SemanticTokensLegend = { tokenTypes: [], tokenModifiers: [] };

	private async _provideSemanticTokens(model: ITextModel): Promise<SemanticTokens | null> {
		try {
			const result = await this._request<any>('provideSemanticTokens', {
				uri: model.uri.toString(),
				languageId: model.getLanguageId(),
				version: model.getVersionId(),
			});
			if (!result?.data) {
				return null;
			}
			return {
				data: new Uint32Array(result.data),
				resultId: result.resultId,
			};
		} catch {
			return null;
		}
	}

	private async _provideDocumentColors(model: ITextModel): Promise<IColorInformation[] | null> {
		try {
			const result = await this._request<any[]>('provideDocumentColors', {
				uri: model.uri.toString(),
				languageId: model.getLanguageId(),
				version: model.getVersionId(),
			});
			if (!Array.isArray(result) || !result.length) {
				return null;
			}
			return result.map(c => ({
				range: toVscRange(c.range),
				color: { red: c.color.red, green: c.color.green, blue: c.color.blue, alpha: c.color.alpha },
			}));
		} catch {
			return null;
		}
	}

	// ── Conversion Helpers ────────────────────────────────────────────────────

	private _convertCompletionItem(item: any): CompletionItem {
		return {
			label: item.label,
			kind: item.kind ?? CompletionItemKind.Text,
			detail: item.detail,
			documentation: item.documentation ? { value: String(item.documentation) } : undefined,
			insertText: item.insertText ?? item.label,
			range: item.range ? toVscRange(item.range) : undefined,
			sortText: item.sortText,
			filterText: item.filterText,
			preselect: item.preselect,
		} as CompletionItem;
	}

	private _convertLocation(loc: any): Location {
		return {
			uri: URI.parse(loc.uri),
			range: toVscRange(loc.range),
		};
	}

	private _convertLocations(result: any): Location | Location[] {
		return Array.isArray(result)
			? result.map(l => this._convertLocation(l))
			: this._convertLocation(result);
	}

	private _convertDocumentSymbol(s: any): DocumentSymbol {
		return {
			name: s.name,
			detail: s.detail || '',
			kind: s.kind ?? SymbolKind.Variable,
			tags: [],
			range: toVscRange(s.range),
			selectionRange: toVscRange(s.selectionRange || s.range),
			children: (s.children || []).map((c: any) => this._convertDocumentSymbol(c)),
		} as DocumentSymbol;
	}

	private _convertWorkspaceEdit(edit: any): WorkspaceEdit & Rejection {
		const edits: IWorkspaceTextEdit[] = (edit.edits || []).map((e: any) => ({
			resource: URI.parse(e.uri),
			versionId: undefined,
			textEdit: {
				range: toVscRange(e.range),
				text: e.newText,
			},
		}));
		return { edits } as WorkspaceEdit & Rejection;
	}
}

registerWorkbenchContribution2(
	TauriExtensionHostContribution.ID,
	TauriExtensionHostContribution,
	WorkbenchPhase.AfterRestored,
);
