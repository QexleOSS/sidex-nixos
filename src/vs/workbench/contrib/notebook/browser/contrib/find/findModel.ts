/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the MIT License. See License.txt in the project root for license information.
 *--------------------------------------------------------------------------------------------*/

export class CellFindMatchModel {
	readonly cell: any;
	readonly index: number;
	readonly contentMatches: any[];
	readonly webviewMatches: any[];
	readonly modelMatchCount: number;

	constructor(cell: any, index: number, contentMatches: any[], webviewMatches: any[]) {
		this.cell = cell;
		this.index = index;
		this.contentMatches = contentMatches;
		this.webviewMatches = webviewMatches;
		this.modelMatchCount = contentMatches.length + webviewMatches.length;
	}

	get length(): number {
		return this.modelMatchCount;
	}

	getMatch(index: number): any {
		if (index < this.contentMatches.length) {
			return this.contentMatches[index];
		}
		return this.webviewMatches[index - this.contentMatches.length];
	}
}
