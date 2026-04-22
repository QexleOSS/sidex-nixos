/**
 * Top queries the Worker pre-warms on a schedule so that the most
 * common searches always hit cache, even on a cold POP.
 *
 * Ordering is roughly by global install counts from the Open VSX /
 * MS Marketplace "trending" lists. Keeping this list small
 * (<30 entries) matters — each prefetch costs one upstream fan-out,
 * and Cloudflare caps scheduled CPU time at 30s per invocation.
 */
export const TOP_QUERIES: ReadonlyArray<string> = [
	'', // empty query = default listing
	'python',
	'rust',
	'typescript',
	'javascript',
	'go',
	'java',
	'c++',
	'docker',
	'git',
	'vim',
	'emacs',
	'markdown',
	'eslint',
	'prettier',
	'tailwind',
	'react',
	'vue',
	'svelte',
	'nextjs',
	'rust-analyzer',
	'gitlens',
	'copilot',
	'live share',
	'remote ssh',
	'wsl',
	'github',
	'kubernetes',
	'terraform',
	'shell'
];
