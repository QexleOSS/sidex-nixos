/*---------------------------------------------------------------------------------------------
 *  SideX: Stub for removed notebook execution state service types.
 *--------------------------------------------------------------------------------------------*/

export interface ICellExecutionStateUpdate {
	editType: number;
	executionOrder?: number;
	runStartTime?: number;
}

export interface ICellExecutionComplete {
	runEndTime?: number;
	lastRunSuccess?: boolean;
}
