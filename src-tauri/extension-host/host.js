'use strict';

const fs = require('fs');
const path = require('path');
const { EventEmitter } = require('events');

// ── Extension Host ──────────────────────────────────────────────────────
// Loads, activates, and manages Node.js VS Code extensions.
// Implements enough of the `vscode` API surface for typical language
// extensions (diagnostics, completions, hovers, etc.) to function.

class ExtensionHost extends EventEmitter {
  constructor() {
    super();
    this._extensions = new Map();     // id → { manifest, module, context, exports }
    this._diagnostics = new Map();    // uri → [{ range, message, severity }]
    this._commands = new Map();       // command-id → handler fn
    this._reqId = 0;
    this._pendingRequests = new Map();
    this._disposables = [];
    this._extensionPaths = [];
  }

  // ── Lifecycle ───────────────────────────────────────────────────────

  initialize() {
    this._registerBuiltinCommands();
    log('host initialized');
  }

  shutdown() {
    for (const [id, ext] of this._extensions) {
      try {
        if (ext.exports && typeof ext.exports.deactivate === 'function') {
          const result = ext.exports.deactivate();
          if (result && typeof result.then === 'function') {
            result.catch((e) => log(`deactivate error (${id}): ${e.message}`));
          }
        }
      } catch (e) {
        log(`deactivate error (${id}): ${e.message}`);
      }
    }
    this._extensions.clear();
    log('host shut down');
  }

  // ── Message router (called by server.js) ──────────────────────────

  handleMessage(msg) {
    const { id, type, method, params } = msg;

    switch (type || method) {
      case 'ping':
        return { id, type: 'pong' };

      case 'initialize':
        return this._handleInitialize(id, params);

      case 'loadExtension':
        return this._handleLoadExtension(id, params);

      case 'activateExtension':
        return this._handleActivateExtension(id, params);

      case 'deactivateExtension':
        return this._handleDeactivateExtension(id, params);

      case 'executeCommand':
        return this._handleExecuteCommand(id, params);

      case 'provideCompletionItems':
        return this._handleProvideCompletionItems(id, params);

      case 'provideHover':
        return this._handleProvideHover(id, params);

      case 'provideDefinition':
        return this._handleProvideDefinition(id, params);

      case 'provideReferences':
        return this._handleProvideReferences(id, params);

      case 'provideDocumentSymbols':
        return this._handleProvideDocumentSymbols(id, params);

      case 'listExtensions':
        return this._handleListExtensions(id);

      case 'getDiagnostics':
        return this._handleGetDiagnostics(id, params);

      default:
        return { id, error: `unknown method: ${type || method}` };
    }
  }

  // ── Protocol handlers ─────────────────────────────────────────────

  _handleInitialize(id, params) {
    if (params && params.extensionPaths) {
      this._extensionPaths = params.extensionPaths;
    }
    return {
      id,
      result: {
        capabilities: [
          'completionProvider',
          'hoverProvider',
          'definitionProvider',
          'referencesProvider',
          'documentSymbolProvider',
          'diagnostics',
          'commands',
        ],
      },
    };
  }

  _handleLoadExtension(id, params) {
    try {
      const { extensionPath } = params;
      const manifest = this._readManifest(extensionPath);
      this._extensions.set(manifest.id, {
        manifest,
        extensionPath,
        module: null,
        context: null,
        exports: null,
        activated: false,
      });
      return { id, result: { extensionId: manifest.id, name: manifest.name } };
    } catch (e) {
      return { id, error: e.message };
    }
  }

  _handleActivateExtension(id, params) {
    try {
      const { extensionId } = params;
      this._activateExtension(extensionId);
      return { id, result: { activated: true } };
    } catch (e) {
      return { id, error: e.message };
    }
  }

  _handleDeactivateExtension(id, params) {
    try {
      const { extensionId } = params;
      const ext = this._extensions.get(extensionId);
      if (!ext) throw new Error(`extension not found: ${extensionId}`);

      if (ext.exports && typeof ext.exports.deactivate === 'function') {
        ext.exports.deactivate();
      }
      ext.activated = false;
      return { id, result: { deactivated: true } };
    } catch (e) {
      return { id, error: e.message };
    }
  }

  _handleExecuteCommand(id, params) {
    const { command, args } = params;
    const handler = this._commands.get(command);
    if (!handler) return { id, error: `unknown command: ${command}` };

    try {
      const result = handler(...(args || []));
      if (result && typeof result.then === 'function') {
        result
          .then((r) => this.emit('event', { id, result: r ?? null }))
          .catch((e) => this.emit('event', { id, error: e.message }));
        return undefined; // async — reply sent via event
      }
      return { id, result: result ?? null };
    } catch (e) {
      return { id, error: e.message };
    }
  }

  _handleProvideCompletionItems(id, params) {
    // Placeholder — real providers registered by activated extensions
    return { id, result: { items: [] } };
  }

  _handleProvideHover(id, params) {
    return { id, result: null };
  }

  _handleProvideDefinition(id, params) {
    return { id, result: null };
  }

  _handleProvideReferences(id, params) {
    return { id, result: [] };
  }

  _handleProvideDocumentSymbols(id, params) {
    return { id, result: [] };
  }

  _handleListExtensions(id) {
    const list = [];
    for (const [extId, ext] of this._extensions) {
      list.push({
        id: extId,
        name: ext.manifest.name,
        version: ext.manifest.version,
        activated: ext.activated,
      });
    }
    return { id, result: list };
  }

  _handleGetDiagnostics(id, params) {
    const uri = params && params.uri;
    if (uri) {
      return { id, result: this._diagnostics.get(uri) || [] };
    }
    const all = {};
    for (const [u, diags] of this._diagnostics) {
      all[u] = diags;
    }
    return { id, result: all };
  }

  // ── Extension loading internals ───────────────────────────────────

  _readManifest(extensionPath) {
    const pkgPath = path.join(extensionPath, 'package.json');
    if (!fs.existsSync(pkgPath)) {
      throw new Error(`no package.json at ${extensionPath}`);
    }
    const raw = JSON.parse(fs.readFileSync(pkgPath, 'utf-8'));
    const publisher = raw.publisher || 'unknown';
    const name = raw.name || path.basename(extensionPath);
    return {
      id: `${publisher}.${name}`,
      name: raw.displayName || name,
      version: raw.version || '0.0.0',
      main: raw.main,
      activationEvents: raw.activationEvents || [],
      contributes: raw.contributes || {},
    };
  }

  _activateExtension(extensionId) {
    const ext = this._extensions.get(extensionId);
    if (!ext) throw new Error(`extension not found: ${extensionId}`);
    if (ext.activated) return;

    if (!ext.manifest.main) {
      ext.activated = true;
      return;
    }

    const mainPath = path.resolve(ext.extensionPath, ext.manifest.main);
    const context = this._createExtensionContext(extensionId, ext.extensionPath);

    const mod = require(mainPath);
    ext.module = mod;
    ext.context = context;

    if (typeof mod.activate === 'function') {
      ext.exports = mod.activate(context) || mod;
    } else {
      ext.exports = mod;
    }
    ext.activated = true;
    log(`activated ${extensionId}`);
  }

  // ── Minimal vscode API shim ───────────────────────────────────────

  _createExtensionContext(extensionId, extensionPath) {
    const subscriptions = [];
    const host = this;

    return {
      extensionPath,
      extensionUri: { fsPath: extensionPath, scheme: 'file', path: extensionPath },
      storagePath: path.join(extensionPath, '.storage'),
      globalStoragePath: path.join(extensionPath, '.global-storage'),
      subscriptions,
      asAbsolutePath: (rel) => path.join(extensionPath, rel),
    };
  }

  _registerBuiltinCommands() {
    this._commands.set('sidex.extHost.ping', () => 'pong');

    this._commands.set('sidex.extHost.listLoaded', () => {
      const list = [];
      for (const [id, ext] of this._extensions) {
        list.push({ id, activated: ext.activated });
      }
      return list;
    });
  }
}

// ── Fake `vscode` module ────────────────────────────────────────────────
// Injected into Node's module resolution so `require('vscode')` works
// inside extensions.

let hostInstance = null;

function createVscodeShim() {
  const host = hostInstance;

  const DiagnosticSeverity = { Error: 0, Warning: 1, Information: 2, Hint: 3 };
  const CompletionItemKind = {
    Text: 0, Method: 1, Function: 2, Constructor: 3, Field: 4,
    Variable: 5, Class: 6, Interface: 7, Module: 8, Property: 9,
    Unit: 10, Value: 11, Enum: 12, Keyword: 13, Snippet: 14,
    Color: 15, File: 16, Reference: 17, Folder: 18,
  };
  const SymbolKind = {
    File: 0, Module: 1, Namespace: 2, Package: 3, Class: 4,
    Method: 5, Property: 6, Field: 7, Constructor: 8, Enum: 9,
    Interface: 10, Function: 11, Variable: 12, Constant: 13,
    String: 14, Number: 15, Boolean: 16, Array: 17, Object: 18,
    Key: 19, Null: 20, EnumMember: 21, Struct: 22, Event: 23,
    Operator: 24, TypeParameter: 25,
  };

  class Position {
    constructor(line, character) { this.line = line; this.character = character; }
  }
  class Range {
    constructor(startLine, startChar, endLine, endChar) {
      if (startLine instanceof Position) {
        this.start = startLine;
        this.end = startChar;
      } else {
        this.start = new Position(startLine, startChar);
        this.end = new Position(endLine, endChar);
      }
    }
  }
  class Location {
    constructor(uri, rangeOrPos) { this.uri = uri; this.range = rangeOrPos; }
  }
  class Diagnostic {
    constructor(range, message, severity) {
      this.range = range;
      this.message = message;
      this.severity = severity ?? DiagnosticSeverity.Error;
    }
  }
  class Uri {
    constructor(scheme, authority, p) {
      this.scheme = scheme || 'file';
      this.authority = authority || '';
      this.path = p || '';
      this.fsPath = p || '';
    }
    static file(p) { return new Uri('file', '', p); }
    static parse(s) {
      try {
        const u = new URL(s);
        return new Uri(u.protocol.replace(':', ''), u.host, u.pathname);
      } catch {
        return new Uri('file', '', s);
      }
    }
    toString() { return `${this.scheme}://${this.authority}${this.path}`; }
  }
  class CompletionItem {
    constructor(label, kind) { this.label = label; this.kind = kind; }
  }
  class CompletionList {
    constructor(items, isIncomplete) {
      this.items = items || [];
      this.isIncomplete = !!isIncomplete;
    }
  }

  const diagnosticCollections = new Map();

  class DiagnosticCollection {
    constructor(name) {
      this.name = name;
      this._entries = new Map();
    }
    set(uri, diagnostics) {
      const key = typeof uri === 'string' ? uri : uri.toString();
      this._entries.set(key, diagnostics || []);
      host._diagnostics.set(key, (diagnostics || []).map((d) => ({
        range: d.range,
        message: d.message,
        severity: d.severity,
      })));
      host.emit('event', { type: 'diagnosticsChanged', uri: key });
    }
    delete(uri) {
      const key = typeof uri === 'string' ? uri : uri.toString();
      this._entries.delete(key);
      host._diagnostics.delete(key);
      host.emit('event', { type: 'diagnosticsChanged', uri: key });
    }
    clear() {
      for (const key of this._entries.keys()) {
        host._diagnostics.delete(key);
      }
      this._entries.clear();
      host.emit('event', { type: 'diagnosticsChanged' });
    }
    dispose() { this.clear(); diagnosticCollections.delete(this.name); }
  }

  const noopDisposable = { dispose() {} };
  const emptyEvent = (_listener) => noopDisposable;

  const languages = {
    createDiagnosticCollection(name) {
      const col = new DiagnosticCollection(name || 'default');
      diagnosticCollections.set(col.name, col);
      return col;
    },
    registerCompletionItemProvider: () => noopDisposable,
    registerHoverProvider: () => noopDisposable,
    registerDefinitionProvider: () => noopDisposable,
    registerReferenceProvider: () => noopDisposable,
    registerDocumentSymbolProvider: () => noopDisposable,
    registerCodeActionsProvider: () => noopDisposable,
    registerCodeLensProvider: () => noopDisposable,
    registerDocumentFormattingEditProvider: () => noopDisposable,
    registerDocumentRangeFormattingEditProvider: () => noopDisposable,
    registerSignatureHelpProvider: () => noopDisposable,
    registerDocumentHighlightProvider: () => noopDisposable,
    registerRenameProvider: () => noopDisposable,
    getDiagnostics: () => [],
    match: () => 0,
  };

  const commands = {
    registerCommand(id, handler) {
      host._commands.set(id, handler);
      return { dispose() { host._commands.delete(id); } };
    },
    executeCommand(id, ...args) {
      const fn = host._commands.get(id);
      if (fn) return Promise.resolve(fn(...args));
      return Promise.reject(new Error(`command not found: ${id}`));
    },
    getCommands: () => Promise.resolve([...host._commands.keys()]),
  };

  const workspace = {
    workspaceFolders: [],
    getConfiguration: () => ({
      get: () => undefined,
      has: () => false,
      update: () => Promise.resolve(),
      inspect: () => undefined,
    }),
    onDidChangeConfiguration: emptyEvent,
    onDidOpenTextDocument: emptyEvent,
    onDidCloseTextDocument: emptyEvent,
    onDidChangeTextDocument: emptyEvent,
    onDidSaveTextDocument: emptyEvent,
    createFileSystemWatcher: () => ({
      onDidCreate: emptyEvent,
      onDidChange: emptyEvent,
      onDidDelete: emptyEvent,
      dispose() {},
    }),
    fs: {
      readFile: (uri) => fs.promises.readFile(uri.fsPath || uri.path),
      writeFile: (uri, content) => fs.promises.writeFile(uri.fsPath || uri.path, content),
      stat: (uri) => fs.promises.stat(uri.fsPath || uri.path),
    },
    openTextDocument: () => Promise.resolve({ getText: () => '', uri: Uri.file('') }),
    findFiles: () => Promise.resolve([]),
  };

  const window = {
    showInformationMessage: (msg) => { log(`[info] ${msg}`); return Promise.resolve(undefined); },
    showWarningMessage: (msg) => { log(`[warn] ${msg}`); return Promise.resolve(undefined); },
    showErrorMessage: (msg) => { log(`[error] ${msg}`); return Promise.resolve(undefined); },
    createOutputChannel: (name) => ({
      name,
      append: () => {},
      appendLine: (line) => { log(`[${name}] ${line}`); },
      clear: () => {},
      show: () => {},
      hide: () => {},
      dispose: () => {},
    }),
    createStatusBarItem: () => ({
      text: '', tooltip: '', command: '', show() {}, hide() {}, dispose() {},
    }),
    showQuickPick: () => Promise.resolve(undefined),
    showInputBox: () => Promise.resolve(undefined),
    activeTextEditor: undefined,
    visibleTextEditors: [],
    onDidChangeActiveTextEditor: emptyEvent,
    onDidChangeVisibleTextEditors: emptyEvent,
    createTextEditorDecorationType: () => ({ dispose() {} }),
    withProgress: (_opts, task) => task({ report() {} }, { isCancellationRequested: false }),
  };

  const extensions = {
    getExtension: (id) => {
      const ext = host._extensions.get(id);
      if (!ext) return undefined;
      return { id, extensionPath: ext.extensionPath, exports: ext.exports, isActive: ext.activated };
    },
    all: [],
    onDidChange: emptyEvent,
  };

  const env = {
    appName: 'SideX',
    appRoot: process.cwd(),
    language: 'en',
    machineId: 'sidex',
    sessionId: crypto.randomUUID ? crypto.randomUUID() : `${Date.now()}`,
    uriScheme: 'sidex',
  };

  return {
    // Types & enums
    Position, Range, Location, Uri, Diagnostic,
    DiagnosticSeverity, CompletionItem, CompletionItemKind, CompletionList, SymbolKind,
    // Namespaces
    languages, commands, workspace, window, extensions, env,
    // Misc stubs
    EventEmitter: require('events').EventEmitter,
    CancellationTokenSource: class { constructor() { this.token = { isCancellationRequested: false }; } cancel() { this.token.isCancellationRequested = true; } dispose() {} },
    Disposable: class { constructor(fn) { this._fn = fn; } dispose() { if (this._fn) { this._fn(); this._fn = null; } } },
    TreeItem: class { constructor(label) { this.label = label; } },
    ThemeIcon: class { constructor(id) { this.id = id; } },
    MarkdownString: class { constructor(value) { this.value = value || ''; } },
    StatusBarAlignment: { Left: 1, Right: 2 },
    ViewColumn: { Active: -1, Beside: -2, One: 1, Two: 2, Three: 3 },
    EndOfLine: { LF: 1, CRLF: 2 },
    TextEditorRevealType: { Default: 0, InCenter: 1 },
    OverviewRulerLane: { Left: 1, Center: 2, Right: 4 },
    ConfigurationTarget: { Global: 1, Workspace: 2, WorkspaceFolder: 3 },
  };
}

// Register the shim so `require('vscode')` resolves inside extensions
function installVscodeShim() {
  const Module = require('module');
  const original = Module._resolveFilename;
  Module._resolveFilename = function (request, parent, isMain, options) {
    if (request === 'vscode') {
      return '__sidex_vscode_shim__';
    }
    return original.call(this, request, parent, isMain, options);
  };

  const originalLoad = Module._cache;
  require.cache['__sidex_vscode_shim__'] = {
    id: '__sidex_vscode_shim__',
    filename: '__sidex_vscode_shim__',
    loaded: true,
    exports: null, // filled after host is created
  };
}

// ── Module exports (consumed by server.js) ──────────────────────────────

function log(msg) {
  process.stderr.write(`[ext-host] ${msg}\n`);
}

installVscodeShim();

hostInstance = new ExtensionHost();

require.cache['__sidex_vscode_shim__'].exports = createVscodeShim();

module.exports = hostInstance;
