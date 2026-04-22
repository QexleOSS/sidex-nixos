/*---------------------------------------------------------------------------------------------
 *  SideX: Stub for removed timeline types.
 *--------------------------------------------------------------------------------------------*/

import { URI } from '../../../../base/common/uri.js';
import { Event } from '../../../../base/common/event.js';
import { createDecorator } from '../../../../platform/instantiation/common/instantiation.js';

export interface TimelineItem {
	handle: string;
	source: string;
	id?: string;
	label: string;
	description?: string;
	tooltip?: string;
	timestamp: number;
	accessibilityInformation?: { label: string; role?: string };
	icon?: URI;
	iconDark?: URI;
	themeIcon?: { id: string; color?: { id: string } };
	command?: { id: string; title: string; arguments?: unknown[] };
	contextValue?: string;
}

export interface TimelineOptions {
	cursor?: string;
	limit?: number | { timestamp: number; id?: string };
}

export interface TimelineChangeEvent {
	id: string;
	uri: URI | undefined;
	reset: boolean;
}

export interface Timeline {
	source: string;
	items: TimelineItem[];
	paging?: { cursor: string | undefined };
}

export interface TimelineProvider {
	id: string;
	label: string;
	scheme: string | string[];
	onDidChange?: Event<TimelineChangeEvent>;
	provideTimeline(uri: URI, options: TimelineOptions, token: unknown): Promise<Timeline | undefined>;
	dispose(): void;
}

export interface TimelineProviderDescriptor {
	id: string;
	label: string;
	scheme: string | string[];
}

export const ITimelineService = createDecorator<ITimelineService>('timelineService');
export interface ITimelineService {
	readonly _serviceBrand: undefined;
	onDidChangeProviders: Event<void>;
	onDidChangeTimeline: Event<TimelineChangeEvent>;
	onDidChangeUri: Event<URI>;
	registerTimelineProvider(provider: TimelineProvider): void;
	unregisterTimelineProvider(id: string): void;
	getSources(): TimelineProviderDescriptor[];
	getTimeline(id: string, uri: URI, options: TimelineOptions, token: unknown): Promise<Timeline | undefined>;
}
