'use strict';

const http = require('http');
const net = require('net');
const path = require('path');
const fs = require('fs');
const os = require('os');
const crypto = require('crypto');
const cp = require('child_process');

// ── RFC 6455 WebSocket helpers (zero dependencies) ──────────────────────

const WS_MAGIC = '258EAFA5-E914-47DA-95CA-C5AB0DC85B11';
function acceptKey(key) {
	return crypto.createHash('sha1').update(key + WS_MAGIC).digest('base64');
}

function decodeFrame(buf) {
	if (buf.length < 2) return null;
	const secondByte = buf[1];
	const masked = (secondByte & 0x80) !== 0;
	let payloadLen = secondByte & 0x7f;
	let offset = 2;
	if (payloadLen === 126) {
		if (buf.length < 4) return null;
		payloadLen = buf.readUInt16BE(2);
		offset = 4;
	} else if (payloadLen === 127) {
		if (buf.length < 10) return null;
		payloadLen = Number(buf.readBigUInt64BE(2));
		offset = 10;
	}
	let maskKey = null;
	if (masked) {
		if (buf.length < offset + 4) return null;
		maskKey = buf.slice(offset, offset + 4);
		offset += 4;
	}
	if (buf.length < offset + payloadLen) return null;
	const payload = buf.slice(offset, offset + payloadLen);
	if (maskKey) {
		for (let i = 0; i < payload.length; i++) payload[i] ^= maskKey[i & 3];
	}
	return { opcode: buf[0] & 0x0f, payload, totalLength: offset + payloadLen };
}

function encodeFrame(opcode, data) {
	const payload = Buffer.isBuffer(data) ? data : Buffer.from(data, 'utf-8');
	const len = payload.length;
	let header;
	if (len < 126) {
		header = Buffer.alloc(2);
		header[0] = 0x80 | opcode;
		header[1] = len;
	} else if (len < 65536) {
		header = Buffer.alloc(4);
		header[0] = 0x80 | opcode;
		header[1] = 126;
		header.writeUInt16BE(len, 2);
	} else {
		header = Buffer.alloc(10);
		header[0] = 0x80 | opcode;
		header[1] = 127;
		header.writeBigUInt64BE(BigInt(len), 2);
	}
	return Buffer.concat([header, payload]);
}

// ── Constants ───────────────────────────────────────────────────────────

const SIDEX_DATA_DIR = path.join(os.homedir(), '.sidex');
const EXTENSIONS_DIR = path.join(SIDEX_DATA_DIR, 'extensions');
const USER_DATA_DIR = path.join(SIDEX_DATA_DIR, 'data');
const GLOBAL_STORAGE_DIR = path.join(USER_DATA_DIR, 'User', 'globalStorage');

[EXTENSIONS_DIR, USER_DATA_DIR, GLOBAL_STORAGE_DIR].forEach(d => {
	try { if (!fs.existsSync(d)) fs.mkdirSync(d, { recursive: true }); } catch {}
});

// ── Extension scanner ───────────────────────────────────────────────────

function scanExtensions(searchPaths) {
	const extensions = [];
	for (const searchPath of searchPaths) {
		try {
			if (!fs.existsSync(searchPath)) continue;
			const entries = fs.readdirSync(searchPath, { withFileTypes: true });
			for (const entry of entries) {
				if (!entry.isDirectory()) continue;
				const extDir = path.join(searchPath, entry.name);
				const pkgPath = path.join(extDir, 'package.json');
				if (!fs.existsSync(pkgPath)) continue;
				try {
					const raw = JSON.parse(fs.readFileSync(pkgPath, 'utf-8'));
					const publisher = raw.publisher || 'unknown';
					const name = raw.name || entry.name;
					const id = `${publisher}.${name}`;
					extensions.push({
						identifier: { id, uuid: undefined },
						location: { scheme: 'file', path: extDir },
						extensionLocation: { scheme: 'file', path: extDir, authority: '' },
						packageJSON: raw,
						isBuiltin: false,
						isUnderDevelopment: false,
						metadata: undefined,
						targetPlatform: 'undefined',
					});
				} catch (e) {
					log(`skip ${entry.name}: ${e.message}`);
				}
			}
		} catch (e) {
			log(`scan error ${searchPath}: ${e.message}`);
		}
	}
	return extensions;
}

function getExtensionSearchPaths() {
	const paths = [EXTENSIONS_DIR];
	const localExt = path.join(process.cwd(), 'extensions');
	if (localExt !== EXTENSIONS_DIR) paths.push(localExt);
	const tauriExt = path.join(__dirname, '..', 'extensions');
	paths.push(tauriExt);
	return paths;
}

// ── VSCode Protocol Constants ───────────────────────────────────────────

const VSCodeConnectionType = {
	Management: 1,
	ExtensionHost: 2,
	Tunnel: 3,
};

const MessageType = {
	None: 0,
	Regular: 1,
	Control: 2,
	Ack: 3,
	Disconnect: 5,
	ReplayRequest: 6,
	Pause: 7,
	Resume: 8,
};

function createVSCodeMessage(type) {
	const buf = Buffer.alloc(1);
	buf[0] = type;
	return buf;
}

function isVSCodeMessageType(buf, type) {
	return buf.length === 1 && buf[0] === type;
}

// ── Extension Host Process Management ───────────────────────────────────

class ExtensionHostManager {
	constructor() {
		this._processes = new Map();
		this._connectionToken = crypto.randomUUID();
	}

	get connectionToken() {
		return this._connectionToken;
	}

	createExtensionHostProcess(reconnectionToken, initData) {
		if (this._processes.has(reconnectionToken)) {
			return this._processes.get(reconnectionToken);
		}

		const hostPath = path.join(__dirname, 'host.cjs');
		const child = cp.fork(hostPath, ['--type=extensionHost'], {
			silent: true,
			env: {
				...process.env,
				VSCODE_HANDLES_UNCAUGHT_ERRORS: 'true',
				SIDEX_EXTENSION_HOST: 'true',
				SIDEX_INIT_DATA: JSON.stringify(initData),
			}
		});

		const entry = { child, reconnectionToken, initData };
		this._processes.set(reconnectionToken, entry);

		child.on('exit', (code, signal) => {
			log(`ext-host process <${child.pid}> exited: code=${code} signal=${signal}`);
			this._processes.delete(reconnectionToken);
		});

		child.on('error', (err) => {
			log(`ext-host process error: ${err.message}`);
			this._processes.delete(reconnectionToken);
		});

		if (child.stdout) {
			child.stdout.setEncoding('utf8');
			child.stdout.on('data', d => log(`<${child.pid}> ${d.trimEnd()}`));
		}
		if (child.stderr) {
			child.stderr.setEncoding('utf8');
			child.stderr.on('data', d => log(`<${child.pid}><stderr> ${d.trimEnd()}`));
		}

		log(`spawned ext-host process <${child.pid}>`);
		return entry;
	}

	shutdown() {
		for (const [, entry] of this._processes) {
			try { entry.child.kill(); } catch {}
		}
		this._processes.clear();
	}
}

// ── WebSocket Connection Handler ────────────────────────────────────────

const hostManager = new ExtensionHostManager();

function handleUpgrade(req, socket) {
	const key = req.headers['sec-websocket-key'];
	if (!key) {
		socket.destroy();
		return;
	}

	const accept = acceptKey(key);
	const protocol = req.headers['sec-websocket-protocol'];
	const protocolHeader = protocol ? `Sec-WebSocket-Protocol: ${protocol}\r\n` : '';

	socket.write(
		'HTTP/1.1 101 Switching Protocols\r\n' +
		'Upgrade: websocket\r\n' +
		'Connection: Upgrade\r\n' +
		`Sec-WebSocket-Accept: ${accept}\r\n` +
		protocolHeader +
		'\r\n'
	);

	const client = new ClientConnection(socket, req.url || '/');
	client.start();
}

class ClientConnection {
	constructor(socket, urlPath) {
		this._socket = socket;
		this._urlPath = urlPath;
		this._buffer = Buffer.alloc(0);
		this._rpcHandlers = new Map();
		this._nextSeq = 1;
		this._pendingReplies = new Map();
		this._extensionHostEntry = null;
		this._disposed = false;
	}

	start() {
		log('client connected');

		this._socket.on('data', (chunk) => this._onData(chunk));
		this._socket.on('close', () => this._onClose());
		this._socket.on('error', (err) => {
			log(`socket error: ${err.message}`);
			this._dispose();
		});

		this._performHandshake();
	}

	_performHandshake() {
		const searchPaths = getExtensionSearchPaths();
		const extensions = scanExtensions(searchPaths);

		log(`discovered ${extensions.length} extensions from ${searchPaths.length} paths`);

		const reconnectionToken = crypto.randomUUID();
		const connectionToken = hostManager.connectionToken;

		const initData = this._buildInitData(extensions, reconnectionToken);

		this._extensionHostEntry = hostManager.createExtensionHostProcess(
			reconnectionToken, initData
		);

		this._setupIPC(this._extensionHostEntry);

		this._sendJson({
			type: 'sidex:handshake',
			connectionToken,
			reconnectionToken,
			extensionCount: extensions.length,
			extensions: extensions.map(e => ({
				id: e.identifier.id,
				location: e.extensionLocation.path,
				name: e.packageJSON.displayName || e.packageJSON.name,
				version: e.packageJSON.version,
				activationEvents: e.packageJSON.activationEvents || [],
				main: e.packageJSON.main,
				browser: e.packageJSON.browser,
				contributes: Object.keys(e.packageJSON.contributes || {}),
			})),
		});
	}

	_buildInitData(extensions, reconnectionToken) {
		return {
			version: '1.90.0',
			commit: undefined,
			parentPid: process.pid,
			environment: {
				isExtensionDevelopmentDebug: false,
				appRoot: process.cwd(),
				appName: 'SideX',
				appHost: 'desktop',
				appUriScheme: 'sidex',
				appLanguage: 'en',
				extensionTelemetryLogResource: { scheme: 'file', path: '' },
				isExtensionTelemetryLoggingOnly: false,
				globalStorageHome: { scheme: 'file', path: GLOBAL_STORAGE_DIR },
				workspaceStorageHome: { scheme: 'file', path: path.join(USER_DATA_DIR, 'workspaceStorage') },
				extensionDevelopmentLocationURI: undefined,
				extensionTestsLocationURI: undefined,
			},
			workspace: undefined,
			remote: { isRemote: false, authority: undefined, connectionData: null },
			extensions: extensions,
			telemetryInfo: {
				sessionId: crypto.randomUUID(),
				machineId: crypto.randomUUID(),
				sqmId: crypto.randomUUID(),
				devDeviceId: crypto.randomUUID(),
				firstSessionDate: new Date().toISOString(),
				commitHash: undefined,
				msftInternal: false,
			},
			logLevel: 2,
			loggers: [],
			logsLocation: { scheme: 'file', path: path.join(USER_DATA_DIR, 'logs') },
			autoStart: true,
			uiKind: 1,
		};
	}

	_setupIPC(entry) {
		const child = entry.child;

		child.on('message', (msg) => {
			if (msg && msg.type === 'VSCODE_EXTHOST_IPC_READY') {
				log(`ext-host <${child.pid}> IPC ready`);
				return;
			}

			if (msg && msg.type === 'sidex:host-event') {
				this._sendJson(msg.event);
				return;
			}

			if (msg && msg.type === 'sidex:host-reply') {
				this._sendJson(msg.reply);
				return;
			}

			if (msg && typeof msg === 'object') {
				this._sendJson({
					type: 'sidex:exthost-message',
					data: msg,
				});
			}
		});

		child.on('exit', () => {
			if (!this._disposed) {
				this._sendJson({ type: 'sidex:exthost-exit' });
			}
		});
	}

	_onData(chunk) {
		this._buffer = Buffer.concat([this._buffer, chunk]);
		while (true) {
			const frame = decodeFrame(this._buffer);
			if (!frame) break;
			this._buffer = this._buffer.slice(frame.totalLength);

			if (frame.opcode === 0x08) {
				this._socket.write(encodeFrame(0x08, Buffer.alloc(0)));
				this._socket.end();
				return;
			}
			if (frame.opcode === 0x09) {
				this._socket.write(encodeFrame(0x0a, frame.payload));
				continue;
			}
			if (frame.opcode === 0x01) {
				this._handleTextMessage(frame.payload.toString('utf-8'));
			}
		}
	}

	_handleTextMessage(text) {
		let msg;
		try {
			msg = JSON.parse(text);
		} catch {
			log('bad JSON from client');
			return;
		}

		const { id, type, method, params } = msg;
		const handler = type || method;

		switch (handler) {
			case 'ping':
				this._sendJson({ id, type: 'pong' });
				break;

			case 'initialize':
				this._handleInitialize(id, params);
				break;

			case 'executeCommand':
				this._forwardToExtHost(id, msg);
				break;

		case 'documentOpened':
		case 'documentChanged':
		case 'documentClosed':
		case 'documentSaved':
		case 'activeEditorChanged':
		case 'messageResponse':
		case 'setLanguageConfiguration':
		case 'fileWatchEvent':
			this._forwardToExtHost(id, msg);
			break;

			case 'provideCompletionItems':
			case 'provideHover':
			case 'provideDefinition':
			case 'provideReferences':
			case 'provideDocumentSymbols':
			case 'provideCodeActions':
			case 'provideCodeLenses':
			case 'provideFormatting':
			case 'provideRangeFormatting':
			case 'provideSignatureHelp':
			case 'provideDocumentHighlight':
			case 'provideRename':
			case 'prepareRename':
			case 'provideDocumentLinks':
			case 'provideInlayHints':
			case 'provideTypeDefinition':
			case 'provideImplementation':
			case 'provideDeclaration':
			case 'provideFoldingRanges':
			case 'provideDocumentColors':
			case 'provideSemanticTokens':
			case 'provideWorkspaceSymbols':
			case 'provideSelectionRanges':
				this._forwardToExtHost(id, msg);
				break;

			case 'discoverExtensions': {
				const paths = (params && params.paths) || getExtensionSearchPaths();
				const extensions = scanExtensions(paths);
				this._sendJson({ id, result: extensions.map(e => ({
					id: e.identifier.id,
					name: e.packageJSON.displayName || e.packageJSON.name,
					path: e.extensionLocation.path,
					activationEvents: e.packageJSON.activationEvents || [],
				})) });
				break;
			}

			case 'listExtensions': {
				const paths = getExtensionSearchPaths();
				const extensions = scanExtensions(paths);
				this._sendJson({ id, result: extensions.map(e => ({
					id: e.identifier.id,
					name: e.packageJSON.displayName || e.packageJSON.name,
					version: e.packageJSON.version,
					activated: false,
				})) });
				break;
			}

			case 'getDiagnostics':
				this._forwardToExtHost(id, msg);
				break;

			case 'setConfiguration':
				this._forwardToExtHost(id, msg);
				break;

			case 'loadExtension':
			case 'activateExtension':
			case 'deactivateExtension':
				this._forwardToExtHost(id, msg);
				break;

			default:
				this._forwardToExtHost(id, msg);
				break;
		}
	}

	_handleInitialize(id, params) {
		this._sendJson({
			id,
			result: {
				capabilities: [
					'completionProvider', 'hoverProvider', 'definitionProvider',
					'referencesProvider', 'documentSymbolProvider', 'diagnostics',
					'commands', 'codeActionProvider', 'codeLensProvider',
					'formattingProvider', 'signatureHelpProvider', 'renameProvider',
					'documentHighlightProvider', 'typeDefinitionProvider',
					'implementationProvider', 'foldingRangeProvider', 'inlayHintProvider',
				],
				connectionToken: hostManager.connectionToken,
				pid: process.pid,
			}
		});
	}

	_forwardToExtHost(id, msg) {
		if (this._extensionHostEntry && this._extensionHostEntry.child.connected) {
			this._extensionHostEntry.child.send({
				...msg,
				_clientId: id,
			});
		} else {
			this._sendJson({ id, error: 'extension host not connected' });
		}
	}

	_sendJson(obj) {
		if (this._disposed) return;
		try {
			this._socket.write(encodeFrame(0x01, JSON.stringify(obj)));
		} catch {}
	}

	_onClose() {
		log('client disconnected');
		this._dispose();
	}

	_dispose() {
		if (this._disposed) return;
		this._disposed = true;
	}
}

// ── HTTP Server ─────────────────────────────────────────────────────────

function findFreePort() {
	return new Promise((resolve, reject) => {
		const srv = net.createServer();
		srv.listen(0, '127.0.0.1', () => {
			const port = srv.address().port;
			srv.close(() => resolve(port));
		});
		srv.on('error', reject);
	});
}

function log(msg) {
	process.stderr.write(`[ext-host] ${msg}\n`);
}

async function main() {
	const port = await findFreePort();

	const server = http.createServer((_req, res) => {
		res.writeHead(200, { 'Content-Type': 'application/json' });
		res.end(JSON.stringify({
			status: 'ok',
			pid: process.pid,
			connectionToken: hostManager.connectionToken,
			extensionPaths: getExtensionSearchPaths(),
		}));
	});

	server.on('upgrade', (req, socket, _head) => {
		handleUpgrade(req, socket);
	});

	server.listen(port, '127.0.0.1', () => {
		process.stdout.write(JSON.stringify({ port }) + '\n');
		log(`listening on 127.0.0.1:${port}`);
	});

	const shutdown = () => {
		log('shutting down');
		hostManager.shutdown();
		server.close();
		process.exit(0);
	};

	process.on('SIGTERM', shutdown);
	process.on('SIGINT', shutdown);
	process.stdin.resume();
	process.stdin.on('end', shutdown);
}

main().catch((err) => {
	process.stderr.write(`[ext-host] fatal: ${err.stack || err}\n`);
	process.exit(1);
});
