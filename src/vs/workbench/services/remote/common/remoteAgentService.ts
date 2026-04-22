/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the MIT License. See License.txt in the project root for license information.
 *--------------------------------------------------------------------------------------------*/

import { createDecorator } from '../../../../platform/instantiation/common/instantiation.js';
import {
	RemoteAgentConnectionContext,
	IRemoteAgentEnvironment
} from '../../../../platform/remote/common/remoteAgentEnvironment.js';
import { IChannel, IServerChannel } from '../../../../base/parts/ipc/common/ipc.js';
import { IDiagnosticInfoOptions, IDiagnosticInfo } from '../../../../platform/diagnostics/common/diagnostics.js';
import { Event } from '../../../../base/common/event.js';
import { ITelemetryData, TelemetryLevel } from '../../../../platform/telemetry/common/telemetry.js';

export type PersistentConnectionEvent = unknown;

export const IRemoteAgentService = createDecorator<IRemoteAgentService>('remoteAgentService');

export interface IRemoteAgentService {
	readonly _serviceBrand: undefined;

	getConnection(): IRemoteAgentConnection | null;
	getEnvironment(): Promise<IRemoteAgentEnvironment | null>;
	getRawEnvironment(): Promise<IRemoteAgentEnvironment | null>;
	getExtensionHostExitInfo(reconnectionToken: string): Promise<IExtensionHostExitInfo | null>;
	getRoundTripTime(): Promise<number | undefined>;
	endConnection(): Promise<void>;
	getDiagnosticInfo(options: IDiagnosticInfoOptions): Promise<IDiagnosticInfo | undefined>;
	updateTelemetryLevel(telemetryLevel: TelemetryLevel): Promise<void>;
	logTelemetry(eventName: string, data?: ITelemetryData): Promise<void>;
	flushTelemetry(): Promise<void>;
}

export interface IExtensionHostExitInfo {
	code: number;
	signal: string;
}

export interface IRemoteAgentConnection {
	readonly remoteAuthority: string;

	readonly onReconnecting: Event<void>;
	readonly onDidStateChange: Event<PersistentConnectionEvent>;

	end(): Promise<void>;
	dispose(): void;
	getChannel<T extends IChannel>(channelName: string): T;
	withChannel<T extends IChannel, R>(channelName: string, callback: (channel: T) => Promise<R>): Promise<R>;
	registerChannel<T extends IServerChannel<RemoteAgentConnectionContext>>(channelName: string, channel: T): void;
	getInitialConnectionTimeMs(): Promise<number>;
	updateGraceTime(graceTime: number): void;
}

export interface IRemoteConnectionLatencyMeasurement {
	readonly initial: number | undefined;
	readonly current: number;
	readonly average: number;
	readonly high: boolean;
}

// Stub: latency measurement now handled by sidex-remote Rust crate
export const remoteConnectionLatencyMeasurer = {
	lastMeasurement: undefined as IRemoteConnectionLatencyMeasurement | undefined,
	get latency() {
		return this.lastMeasurement;
	},
	async measure(_remoteAgentService: IRemoteAgentService): Promise<IRemoteConnectionLatencyMeasurement | undefined> {
		return undefined;
	}
};
