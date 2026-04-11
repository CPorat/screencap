
// this file is generated — do not edit it


/// <reference types="@sveltejs/kit" />

/**
 * This module provides access to environment variables that are injected _statically_ into your bundle at build time and are limited to _private_ access.
 * 
 * |         | Runtime                                                                    | Build time                                                               |
 * | ------- | -------------------------------------------------------------------------- | ------------------------------------------------------------------------ |
 * | Private | [`$env/dynamic/private`](https://svelte.dev/docs/kit/$env-dynamic-private) | [`$env/static/private`](https://svelte.dev/docs/kit/$env-static-private) |
 * | Public  | [`$env/dynamic/public`](https://svelte.dev/docs/kit/$env-dynamic-public)   | [`$env/static/public`](https://svelte.dev/docs/kit/$env-static-public)   |
 * 
 * Static environment variables are [loaded by Vite](https://vitejs.dev/guide/env-and-mode.html#env-files) from `.env` files and `process.env` at build time and then statically injected into your bundle at build time, enabling optimisations like dead code elimination.
 * 
 * **_Private_ access:**
 * 
 * - This module cannot be imported into client-side code
 * - This module only includes variables that _do not_ begin with [`config.kit.env.publicPrefix`](https://svelte.dev/docs/kit/configuration#env) _and do_ start with [`config.kit.env.privatePrefix`](https://svelte.dev/docs/kit/configuration#env) (if configured)
 * 
 * For example, given the following build time environment:
 * 
 * ```env
 * ENVIRONMENT=production
 * PUBLIC_BASE_URL=http://site.com
 * ```
 * 
 * With the default `publicPrefix` and `privatePrefix`:
 * 
 * ```ts
 * import { ENVIRONMENT, PUBLIC_BASE_URL } from '$env/static/private';
 * 
 * console.log(ENVIRONMENT); // => "production"
 * console.log(PUBLIC_BASE_URL); // => throws error during build
 * ```
 * 
 * The above values will be the same _even if_ different values for `ENVIRONMENT` or `PUBLIC_BASE_URL` are set at runtime, as they are statically replaced in your code with their build time values.
 */
declare module '$env/static/private' {
	export const NVM_RC_VERSION: string;
	export const _ZO_DOCTOR: string;
	export const STARSHIP_SHELL: string;
	export const PNPM_UPDATE_NOTIFIER: string;
	export const VSCODE_CRASH_REPORTER_PROCESS_TYPE: string;
	export const XDG_DATA_HOME: string;
	export const npm_config_audit: string;
	export const NODE: string;
	export const NVM_CD_FLAGS: string;
	export const PYENV_ROOT: string;
	export const INIT_CWD: string;
	export const TERM: string;
	export const TF_IN_AUTOMATION: string;
	export const VSCODE_PROCESS_TITLE: string;
	export const CARGO_TERM_PROGRESS_WHEN: string;
	export const HOMEBREW_REPOSITORY: string;
	export const TMPDIR: string;
	export const npm_config_global_prefix: string;
	export const PYTHONUNBUFFERED: string;
	export const CURSOR_WORKSPACE_LABEL: string;
	export const MallocSpaceEfficient: string;
	export const YARN_ENABLE_PROGRESS_BARS: string;
	export const CURSOR_TRACE_ID: string;
	export const MallocNanoZone: string;
	export const YARN_ENABLE_TELEMETRY: string;
	export const NO_COLOR: string;
	export const COLOR: string;
	export const npm_config_noproxy: string;
	export const AWS_PAGER: string;
	export const EXTENSION_KIT_EXTENSION_TYPE: string;
	export const LC_ALL: string;
	export const npm_config_local_prefix: string;
	export const GIT_EDITOR: string;
	export const GIT_TERMINAL_PROMPT: string;
	export const NVM_DIR: string;
	export const USER: string;
	export const COMMAND_MODE: string;
	export const npm_config_globalconfig: string;
	export const SSH_AUTH_SOCK: string;
	export const OMPCODE: string;
	export const __CF_USER_TEXT_ENCODING: string;
	export const npm_execpath: string;
	export const FZF_DEFAULT_OPTS: string;
	export const PAGER: string;
	export const SYSTEMD_PAGER: string;
	export const npm_config_update_notifier: string;
	export const DOTFILES: string;
	export const TF_INPUT: string;
	export const CLOUDSDK_CORE_DISABLE_PROMPTS: string;
	export const HOMEBREW_PAGER: string;
	export const PATH: string;
	export const _: string;
	export const npm_package_json: string;
	export const GLAB_PAGER: string;
	export const __CFBundleIdentifier: string;
	export const npm_config_init_module: string;
	export const npm_config_userconfig: string;
	export const GH_PROMPT_DISABLED: string;
	export const PWD: string;
	export const npm_command: string;
	export const DELTA_PAGER: string;
	export const VSCODE_HANDLES_UNCAUGHT_ERRORS: string;
	export const EDITOR: string;
	export const VSCODE_ESM_ENTRYPOINT: string;
	export const npm_lifecycle_event: string;
	export const CURSOR_AGENT: string;
	export const LANG: string;
	export const npm_package_name: string;
	export const npm_config_progress: string;
	export const XPC_FLAGS: string;
	export const npm_config_npm_version: string;
	export const CURSOR_EXTENSION_HOST_ROLE: string;
	export const PSQL_PAGER: string;
	export const FORCE_COLOR: string;
	export const MACH_PORT_RENDEZVOUS_PEER_VALDATION: string;
	export const npm_config_node_gyp: string;
	export const SSH_ASKPASS: string;
	export const XPC_SERVICE_NAME: string;
	export const npm_package_version: string;
	export const GPG_TTY: string;
	export const npm_config_yes: string;
	export const HOME: string;
	export const MANPAGER: string;
	export const PYENV_SHELL: string;
	export const SHLVL: string;
	export const XDG_CONFIG_HOME: string;
	export const VSCODE_NLS_CONFIG: string;
	export const npm_config_min_release_age: string;
	export const CI: string;
	export const HOMEBREW_PREFIX: string;
	export const GH_PAGER: string;
	export const MYSQL_PAGER: string;
	export const PIP_NO_INPUT: string;
	export const XDG_CACHE_HOME: string;
	export const LESS: string;
	export const LOGNAME: string;
	export const STARSHIP_SESSION_KEY: string;
	export const npm_config_cache: string;
	export const PIP_DISABLE_PIP_VERSION_CHECK: string;
	export const PNPM_DISABLE_SELF_UPDATE_CHECK: string;
	export const VISUAL: string;
	export const npm_lifecycle_script: string;
	export const COMPOSER_NO_INTERACTION: string;
	export const FZF_CTRL_T_COMMAND: string;
	export const OPENCODE_DISABLE_DEFAULT_PLUGINS: string;
	export const VSCODE_CODE_CACHE_PATH: string;
	export const VSCODE_IPC_HOOK: string;
	export const DEBIAN_FRONTEND: string;
	export const FZF_DEFAULT_COMMAND: string;
	export const npm_config_fund: string;
	export const GOPATH: string;
	export const VSCODE_PID: string;
	export const npm_config_user_agent: string;
	export const HOMEBREW_CELLAR: string;
	export const INFOPATH: string;
	export const OSLogRateLimit: string;
	export const GIT_PAGER: string;
	export const BAT_PAGER: string;
	export const CLAUDECODE: string;
	export const VSCODE_CWD: string;
	export const npm_config_prefix: string;
	export const npm_node_execpath: string;
	export const NODE_ENV: string;
}

/**
 * This module provides access to environment variables that are injected _statically_ into your bundle at build time and are _publicly_ accessible.
 * 
 * |         | Runtime                                                                    | Build time                                                               |
 * | ------- | -------------------------------------------------------------------------- | ------------------------------------------------------------------------ |
 * | Private | [`$env/dynamic/private`](https://svelte.dev/docs/kit/$env-dynamic-private) | [`$env/static/private`](https://svelte.dev/docs/kit/$env-static-private) |
 * | Public  | [`$env/dynamic/public`](https://svelte.dev/docs/kit/$env-dynamic-public)   | [`$env/static/public`](https://svelte.dev/docs/kit/$env-static-public)   |
 * 
 * Static environment variables are [loaded by Vite](https://vitejs.dev/guide/env-and-mode.html#env-files) from `.env` files and `process.env` at build time and then statically injected into your bundle at build time, enabling optimisations like dead code elimination.
 * 
 * **_Public_ access:**
 * 
 * - This module _can_ be imported into client-side code
 * - **Only** variables that begin with [`config.kit.env.publicPrefix`](https://svelte.dev/docs/kit/configuration#env) (which defaults to `PUBLIC_`) are included
 * 
 * For example, given the following build time environment:
 * 
 * ```env
 * ENVIRONMENT=production
 * PUBLIC_BASE_URL=http://site.com
 * ```
 * 
 * With the default `publicPrefix` and `privatePrefix`:
 * 
 * ```ts
 * import { ENVIRONMENT, PUBLIC_BASE_URL } from '$env/static/public';
 * 
 * console.log(ENVIRONMENT); // => throws error during build
 * console.log(PUBLIC_BASE_URL); // => "http://site.com"
 * ```
 * 
 * The above values will be the same _even if_ different values for `ENVIRONMENT` or `PUBLIC_BASE_URL` are set at runtime, as they are statically replaced in your code with their build time values.
 */
declare module '$env/static/public' {
	
}

/**
 * This module provides access to environment variables set _dynamically_ at runtime and that are limited to _private_ access.
 * 
 * |         | Runtime                                                                    | Build time                                                               |
 * | ------- | -------------------------------------------------------------------------- | ------------------------------------------------------------------------ |
 * | Private | [`$env/dynamic/private`](https://svelte.dev/docs/kit/$env-dynamic-private) | [`$env/static/private`](https://svelte.dev/docs/kit/$env-static-private) |
 * | Public  | [`$env/dynamic/public`](https://svelte.dev/docs/kit/$env-dynamic-public)   | [`$env/static/public`](https://svelte.dev/docs/kit/$env-static-public)   |
 * 
 * Dynamic environment variables are defined by the platform you're running on. For example if you're using [`adapter-node`](https://github.com/sveltejs/kit/tree/main/packages/adapter-node) (or running [`vite preview`](https://svelte.dev/docs/kit/cli)), this is equivalent to `process.env`.
 * 
 * **_Private_ access:**
 * 
 * - This module cannot be imported into client-side code
 * - This module includes variables that _do not_ begin with [`config.kit.env.publicPrefix`](https://svelte.dev/docs/kit/configuration#env) _and do_ start with [`config.kit.env.privatePrefix`](https://svelte.dev/docs/kit/configuration#env) (if configured)
 * 
 * > [!NOTE] In `dev`, `$env/dynamic` includes environment variables from `.env`. In `prod`, this behavior will depend on your adapter.
 * 
 * > [!NOTE] To get correct types, environment variables referenced in your code should be declared (for example in an `.env` file), even if they don't have a value until the app is deployed:
 * >
 * > ```env
 * > MY_FEATURE_FLAG=
 * > ```
 * >
 * > You can override `.env` values from the command line like so:
 * >
 * > ```sh
 * > MY_FEATURE_FLAG="enabled" npm run dev
 * > ```
 * 
 * For example, given the following runtime environment:
 * 
 * ```env
 * ENVIRONMENT=production
 * PUBLIC_BASE_URL=http://site.com
 * ```
 * 
 * With the default `publicPrefix` and `privatePrefix`:
 * 
 * ```ts
 * import { env } from '$env/dynamic/private';
 * 
 * console.log(env.ENVIRONMENT); // => "production"
 * console.log(env.PUBLIC_BASE_URL); // => undefined
 * ```
 */
declare module '$env/dynamic/private' {
	export const env: {
		NVM_RC_VERSION: string;
		_ZO_DOCTOR: string;
		STARSHIP_SHELL: string;
		PNPM_UPDATE_NOTIFIER: string;
		VSCODE_CRASH_REPORTER_PROCESS_TYPE: string;
		XDG_DATA_HOME: string;
		npm_config_audit: string;
		NODE: string;
		NVM_CD_FLAGS: string;
		PYENV_ROOT: string;
		INIT_CWD: string;
		TERM: string;
		TF_IN_AUTOMATION: string;
		VSCODE_PROCESS_TITLE: string;
		CARGO_TERM_PROGRESS_WHEN: string;
		HOMEBREW_REPOSITORY: string;
		TMPDIR: string;
		npm_config_global_prefix: string;
		PYTHONUNBUFFERED: string;
		CURSOR_WORKSPACE_LABEL: string;
		MallocSpaceEfficient: string;
		YARN_ENABLE_PROGRESS_BARS: string;
		CURSOR_TRACE_ID: string;
		MallocNanoZone: string;
		YARN_ENABLE_TELEMETRY: string;
		NO_COLOR: string;
		COLOR: string;
		npm_config_noproxy: string;
		AWS_PAGER: string;
		EXTENSION_KIT_EXTENSION_TYPE: string;
		LC_ALL: string;
		npm_config_local_prefix: string;
		GIT_EDITOR: string;
		GIT_TERMINAL_PROMPT: string;
		NVM_DIR: string;
		USER: string;
		COMMAND_MODE: string;
		npm_config_globalconfig: string;
		SSH_AUTH_SOCK: string;
		OMPCODE: string;
		__CF_USER_TEXT_ENCODING: string;
		npm_execpath: string;
		FZF_DEFAULT_OPTS: string;
		PAGER: string;
		SYSTEMD_PAGER: string;
		npm_config_update_notifier: string;
		DOTFILES: string;
		TF_INPUT: string;
		CLOUDSDK_CORE_DISABLE_PROMPTS: string;
		HOMEBREW_PAGER: string;
		PATH: string;
		_: string;
		npm_package_json: string;
		GLAB_PAGER: string;
		__CFBundleIdentifier: string;
		npm_config_init_module: string;
		npm_config_userconfig: string;
		GH_PROMPT_DISABLED: string;
		PWD: string;
		npm_command: string;
		DELTA_PAGER: string;
		VSCODE_HANDLES_UNCAUGHT_ERRORS: string;
		EDITOR: string;
		VSCODE_ESM_ENTRYPOINT: string;
		npm_lifecycle_event: string;
		CURSOR_AGENT: string;
		LANG: string;
		npm_package_name: string;
		npm_config_progress: string;
		XPC_FLAGS: string;
		npm_config_npm_version: string;
		CURSOR_EXTENSION_HOST_ROLE: string;
		PSQL_PAGER: string;
		FORCE_COLOR: string;
		MACH_PORT_RENDEZVOUS_PEER_VALDATION: string;
		npm_config_node_gyp: string;
		SSH_ASKPASS: string;
		XPC_SERVICE_NAME: string;
		npm_package_version: string;
		GPG_TTY: string;
		npm_config_yes: string;
		HOME: string;
		MANPAGER: string;
		PYENV_SHELL: string;
		SHLVL: string;
		XDG_CONFIG_HOME: string;
		VSCODE_NLS_CONFIG: string;
		npm_config_min_release_age: string;
		CI: string;
		HOMEBREW_PREFIX: string;
		GH_PAGER: string;
		MYSQL_PAGER: string;
		PIP_NO_INPUT: string;
		XDG_CACHE_HOME: string;
		LESS: string;
		LOGNAME: string;
		STARSHIP_SESSION_KEY: string;
		npm_config_cache: string;
		PIP_DISABLE_PIP_VERSION_CHECK: string;
		PNPM_DISABLE_SELF_UPDATE_CHECK: string;
		VISUAL: string;
		npm_lifecycle_script: string;
		COMPOSER_NO_INTERACTION: string;
		FZF_CTRL_T_COMMAND: string;
		OPENCODE_DISABLE_DEFAULT_PLUGINS: string;
		VSCODE_CODE_CACHE_PATH: string;
		VSCODE_IPC_HOOK: string;
		DEBIAN_FRONTEND: string;
		FZF_DEFAULT_COMMAND: string;
		npm_config_fund: string;
		GOPATH: string;
		VSCODE_PID: string;
		npm_config_user_agent: string;
		HOMEBREW_CELLAR: string;
		INFOPATH: string;
		OSLogRateLimit: string;
		GIT_PAGER: string;
		BAT_PAGER: string;
		CLAUDECODE: string;
		VSCODE_CWD: string;
		npm_config_prefix: string;
		npm_node_execpath: string;
		NODE_ENV: string;
		[key: `PUBLIC_${string}`]: undefined;
		[key: `${string}`]: string | undefined;
	}
}

/**
 * This module provides access to environment variables set _dynamically_ at runtime and that are _publicly_ accessible.
 * 
 * |         | Runtime                                                                    | Build time                                                               |
 * | ------- | -------------------------------------------------------------------------- | ------------------------------------------------------------------------ |
 * | Private | [`$env/dynamic/private`](https://svelte.dev/docs/kit/$env-dynamic-private) | [`$env/static/private`](https://svelte.dev/docs/kit/$env-static-private) |
 * | Public  | [`$env/dynamic/public`](https://svelte.dev/docs/kit/$env-dynamic-public)   | [`$env/static/public`](https://svelte.dev/docs/kit/$env-static-public)   |
 * 
 * Dynamic environment variables are defined by the platform you're running on. For example if you're using [`adapter-node`](https://github.com/sveltejs/kit/tree/main/packages/adapter-node) (or running [`vite preview`](https://svelte.dev/docs/kit/cli)), this is equivalent to `process.env`.
 * 
 * **_Public_ access:**
 * 
 * - This module _can_ be imported into client-side code
 * - **Only** variables that begin with [`config.kit.env.publicPrefix`](https://svelte.dev/docs/kit/configuration#env) (which defaults to `PUBLIC_`) are included
 * 
 * > [!NOTE] In `dev`, `$env/dynamic` includes environment variables from `.env`. In `prod`, this behavior will depend on your adapter.
 * 
 * > [!NOTE] To get correct types, environment variables referenced in your code should be declared (for example in an `.env` file), even if they don't have a value until the app is deployed:
 * >
 * > ```env
 * > MY_FEATURE_FLAG=
 * > ```
 * >
 * > You can override `.env` values from the command line like so:
 * >
 * > ```sh
 * > MY_FEATURE_FLAG="enabled" npm run dev
 * > ```
 * 
 * For example, given the following runtime environment:
 * 
 * ```env
 * ENVIRONMENT=production
 * PUBLIC_BASE_URL=http://example.com
 * ```
 * 
 * With the default `publicPrefix` and `privatePrefix`:
 * 
 * ```ts
 * import { env } from '$env/dynamic/public';
 * console.log(env.ENVIRONMENT); // => undefined, not public
 * console.log(env.PUBLIC_BASE_URL); // => "http://example.com"
 * ```
 * 
 * ```
 * 
 * ```
 */
declare module '$env/dynamic/public' {
	export const env: {
		[key: `PUBLIC_${string}`]: string | undefined;
	}
}
