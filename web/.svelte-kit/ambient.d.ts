
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
	export const _ZO_DOCTOR: string;
	export const OPT_LEVEL: string;
	export const NVM_RC_VERSION: string;
	export const CARGO_PKG_VERSION_PRE: string;
	export const STARSHIP_SHELL: string;
	export const PNPM_UPDATE_NOTIFIER: string;
	export const VSCODE_CRASH_REPORTER_PROCESS_TYPE: string;
	export const CLIPPY_TERMINAL_WIDTH: string;
	export const NODE: string;
	export const npm_config_audit: string;
	export const XDG_DATA_HOME: string;
	export const CARGO_PKG_README: string;
	export const INIT_CWD: string;
	export const PYENV_ROOT: string;
	export const NVM_CD_FLAGS: string;
	export const TF_IN_AUTOMATION: string;
	export const TERM: string;
	export const HOST: string;
	export const CARGO_PKG_HOMEPAGE: string;
	export const VSCODE_PROCESS_TITLE: string;
	export const PROFILE: string;
	export const TMPDIR: string;
	export const HOMEBREW_REPOSITORY: string;
	export const CARGO_TERM_PROGRESS_WHEN: string;
	export const npm_config_global_prefix: string;
	export const CARGO_CFG_TARGET_ENDIAN: string;
	export const PYTHONUNBUFFERED: string;
	export const YARN_ENABLE_PROGRESS_BARS: string;
	export const MallocSpaceEfficient: string;
	export const CURSOR_WORKSPACE_LABEL: string;
	export const YARN_ENABLE_TELEMETRY: string;
	export const MallocNanoZone: string;
	export const CURSOR_TRACE_ID: string;
	export const COLOR: string;
	export const NO_COLOR: string;
	export const npm_config_noproxy: string;
	export const npm_config_local_prefix: string;
	export const LC_ALL: string;
	export const EXTENSION_KIT_EXTENSION_TYPE: string;
	export const CARGO_PKG_NAME: string;
	export const AWS_PAGER: string;
	export const RUSTDOC: string;
	export const OUT_DIR: string;
	export const GIT_TERMINAL_PROMPT: string;
	export const GIT_EDITOR: string;
	export const USER: string;
	export const NVM_DIR: string;
	export const CARGO_CFG_TARGET_FEATURE: string;
	export const CARGO_CFG_TARGET_ABI: string;
	export const RUSTC_WORKSPACE_WRAPPER: string;
	export const COMMAND_MODE: string;
	export const npm_config_globalconfig: string;
	export const CARGO_MANIFEST_DIR: string;
	export const CARGO_ENCODED_RUSTFLAGS: string;
	export const SSH_AUTH_SOCK: string;
	export const __CF_USER_TEXT_ENCODING: string;
	export const OMPCODE: string;
	export const npm_execpath: string;
	export const npm_config_update_notifier: string;
	export const SYSTEMD_PAGER: string;
	export const PAGER: string;
	export const FZF_DEFAULT_OPTS: string;
	export const CLIPPY_ARGS: string;
	export const DOTFILES: string;
	export const CARGO_PKG_REPOSITORY: string;
	export const CARGO_PKG_AUTHORS: string;
	export const CARGO_CFG_TARGET_VENDOR: string;
	export const CARGO_CFG_TARGET_POINTER_WIDTH: string;
	export const TF_INPUT: string;
	export const CLOUDSDK_CORE_DISABLE_PROMPTS: string;
	export const CARGO_CFG_UNIX: string;
	export const PATH: string;
	export const HOMEBREW_PAGER: string;
	export const CARGO_CFG_TARGET_ENV: string;
	export const CARGO_MAKEFLAGS: string;
	export const npm_package_json: string;
	export const _: string;
	export const npm_config_userconfig: string;
	export const npm_config_init_module: string;
	export const __CFBundleIdentifier: string;
	export const GLAB_PAGER: string;
	export const CARGO_PKG_DESCRIPTION: string;
	export const npm_command: string;
	export const PWD: string;
	export const GH_PROMPT_DISABLED: string;
	export const CARGO_PKG_RUST_VERSION: string;
	export const VSCODE_HANDLES_UNCAUGHT_ERRORS: string;
	export const DELTA_PAGER: string;
	export const npm_lifecycle_event: string;
	export const VSCODE_ESM_ENTRYPOINT: string;
	export const EDITOR: string;
	export const npm_package_name: string;
	export const LANG: string;
	export const CURSOR_AGENT: string;
	export const CARGO_PKG_LICENSE_FILE: string;
	export const CARGO: string;
	export const npm_config_progress: string;
	export const CARGO_CFG_TARGET_OS: string;
	export const npm_config_npm_version: string;
	export const XPC_FLAGS: string;
	export const NUM_JOBS: string;
	export const PSQL_PAGER: string;
	export const CURSOR_EXTENSION_HOST_ROLE: string;
	export const CARGO_CFG_FEATURE: string;
	export const MACH_PORT_RENDEZVOUS_PEER_VALDATION: string;
	export const FORCE_COLOR: string;
	export const CARGO_PKG_VERSION_PATCH: string;
	export const npm_config_node_gyp: string;
	export const npm_package_version: string;
	export const XPC_SERVICE_NAME: string;
	export const SSH_ASKPASS: string;
	export const CARGO_PKG_LICENSE: string;
	export const npm_config_yes: string;
	export const GPG_TTY: string;
	export const CARGO_PKG_VERSION_MAJOR: string;
	export const SHLVL: string;
	export const PYENV_SHELL: string;
	export const MANPAGER: string;
	export const HOME: string;
	export const CARGO_FEATURE_DEFAULT: string;
	export const XDG_CONFIG_HOME: string;
	export const npm_config_min_release_age: string;
	export const VSCODE_NLS_CONFIG: string;
	export const HOMEBREW_PREFIX: string;
	export const CI: string;
	export const CARGO_CFG_TARGET_ARCH: string;
	export const GH_PAGER: string;
	export const XDG_CACHE_HOME: string;
	export const PIP_NO_INPUT: string;
	export const MYSQL_PAGER: string;
	export const npm_config_cache: string;
	export const TARGET: string;
	export const STARSHIP_SESSION_KEY: string;
	export const LOGNAME: string;
	export const LESS: string;
	export const npm_lifecycle_script: string;
	export const VISUAL: string;
	export const PNPM_DISABLE_SELF_UPDATE_CHECK: string;
	export const PIP_DISABLE_PIP_VERSION_CHECK: string;
	export const VSCODE_IPC_HOOK: string;
	export const VSCODE_CODE_CACHE_PATH: string;
	export const OPENCODE_DISABLE_DEFAULT_PLUGINS: string;
	export const FZF_CTRL_T_COMMAND: string;
	export const COMPOSER_NO_INTERACTION: string;
	export const CARGO_CFG_DEBUG_ASSERTIONS: string;
	export const npm_config_fund: string;
	export const FZF_DEFAULT_COMMAND: string;
	export const DEBIAN_FRONTEND: string;
	export const CARGO_PKG_VERSION: string;
	export const GOPATH: string;
	export const CARGO_CFG_PANIC: string;
	export const npm_config_user_agent: string;
	export const VSCODE_PID: string;
	export const CARGO_PKG_VERSION_MINOR: string;
	export const CARGO_CFG_TARGET_HAS_ATOMIC: string;
	export const CARGO_CFG_TARGET_FAMILY: string;
	export const INFOPATH: string;
	export const HOMEBREW_CELLAR: string;
	export const CARGO_MANIFEST_PATH: string;
	export const OSLogRateLimit: string;
	export const GIT_PAGER: string;
	export const VSCODE_CWD: string;
	export const RUSTC: string;
	export const DEBUG: string;
	export const CLAUDECODE: string;
	export const BAT_PAGER: string;
	export const npm_node_execpath: string;
	export const npm_config_prefix: string;
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
		_ZO_DOCTOR: string;
		OPT_LEVEL: string;
		NVM_RC_VERSION: string;
		CARGO_PKG_VERSION_PRE: string;
		STARSHIP_SHELL: string;
		PNPM_UPDATE_NOTIFIER: string;
		VSCODE_CRASH_REPORTER_PROCESS_TYPE: string;
		CLIPPY_TERMINAL_WIDTH: string;
		NODE: string;
		npm_config_audit: string;
		XDG_DATA_HOME: string;
		CARGO_PKG_README: string;
		INIT_CWD: string;
		PYENV_ROOT: string;
		NVM_CD_FLAGS: string;
		TF_IN_AUTOMATION: string;
		TERM: string;
		HOST: string;
		CARGO_PKG_HOMEPAGE: string;
		VSCODE_PROCESS_TITLE: string;
		PROFILE: string;
		TMPDIR: string;
		HOMEBREW_REPOSITORY: string;
		CARGO_TERM_PROGRESS_WHEN: string;
		npm_config_global_prefix: string;
		CARGO_CFG_TARGET_ENDIAN: string;
		PYTHONUNBUFFERED: string;
		YARN_ENABLE_PROGRESS_BARS: string;
		MallocSpaceEfficient: string;
		CURSOR_WORKSPACE_LABEL: string;
		YARN_ENABLE_TELEMETRY: string;
		MallocNanoZone: string;
		CURSOR_TRACE_ID: string;
		COLOR: string;
		NO_COLOR: string;
		npm_config_noproxy: string;
		npm_config_local_prefix: string;
		LC_ALL: string;
		EXTENSION_KIT_EXTENSION_TYPE: string;
		CARGO_PKG_NAME: string;
		AWS_PAGER: string;
		RUSTDOC: string;
		OUT_DIR: string;
		GIT_TERMINAL_PROMPT: string;
		GIT_EDITOR: string;
		USER: string;
		NVM_DIR: string;
		CARGO_CFG_TARGET_FEATURE: string;
		CARGO_CFG_TARGET_ABI: string;
		RUSTC_WORKSPACE_WRAPPER: string;
		COMMAND_MODE: string;
		npm_config_globalconfig: string;
		CARGO_MANIFEST_DIR: string;
		CARGO_ENCODED_RUSTFLAGS: string;
		SSH_AUTH_SOCK: string;
		__CF_USER_TEXT_ENCODING: string;
		OMPCODE: string;
		npm_execpath: string;
		npm_config_update_notifier: string;
		SYSTEMD_PAGER: string;
		PAGER: string;
		FZF_DEFAULT_OPTS: string;
		CLIPPY_ARGS: string;
		DOTFILES: string;
		CARGO_PKG_REPOSITORY: string;
		CARGO_PKG_AUTHORS: string;
		CARGO_CFG_TARGET_VENDOR: string;
		CARGO_CFG_TARGET_POINTER_WIDTH: string;
		TF_INPUT: string;
		CLOUDSDK_CORE_DISABLE_PROMPTS: string;
		CARGO_CFG_UNIX: string;
		PATH: string;
		HOMEBREW_PAGER: string;
		CARGO_CFG_TARGET_ENV: string;
		CARGO_MAKEFLAGS: string;
		npm_package_json: string;
		_: string;
		npm_config_userconfig: string;
		npm_config_init_module: string;
		__CFBundleIdentifier: string;
		GLAB_PAGER: string;
		CARGO_PKG_DESCRIPTION: string;
		npm_command: string;
		PWD: string;
		GH_PROMPT_DISABLED: string;
		CARGO_PKG_RUST_VERSION: string;
		VSCODE_HANDLES_UNCAUGHT_ERRORS: string;
		DELTA_PAGER: string;
		npm_lifecycle_event: string;
		VSCODE_ESM_ENTRYPOINT: string;
		EDITOR: string;
		npm_package_name: string;
		LANG: string;
		CURSOR_AGENT: string;
		CARGO_PKG_LICENSE_FILE: string;
		CARGO: string;
		npm_config_progress: string;
		CARGO_CFG_TARGET_OS: string;
		npm_config_npm_version: string;
		XPC_FLAGS: string;
		NUM_JOBS: string;
		PSQL_PAGER: string;
		CURSOR_EXTENSION_HOST_ROLE: string;
		CARGO_CFG_FEATURE: string;
		MACH_PORT_RENDEZVOUS_PEER_VALDATION: string;
		FORCE_COLOR: string;
		CARGO_PKG_VERSION_PATCH: string;
		npm_config_node_gyp: string;
		npm_package_version: string;
		XPC_SERVICE_NAME: string;
		SSH_ASKPASS: string;
		CARGO_PKG_LICENSE: string;
		npm_config_yes: string;
		GPG_TTY: string;
		CARGO_PKG_VERSION_MAJOR: string;
		SHLVL: string;
		PYENV_SHELL: string;
		MANPAGER: string;
		HOME: string;
		CARGO_FEATURE_DEFAULT: string;
		XDG_CONFIG_HOME: string;
		npm_config_min_release_age: string;
		VSCODE_NLS_CONFIG: string;
		HOMEBREW_PREFIX: string;
		CI: string;
		CARGO_CFG_TARGET_ARCH: string;
		GH_PAGER: string;
		XDG_CACHE_HOME: string;
		PIP_NO_INPUT: string;
		MYSQL_PAGER: string;
		npm_config_cache: string;
		TARGET: string;
		STARSHIP_SESSION_KEY: string;
		LOGNAME: string;
		LESS: string;
		npm_lifecycle_script: string;
		VISUAL: string;
		PNPM_DISABLE_SELF_UPDATE_CHECK: string;
		PIP_DISABLE_PIP_VERSION_CHECK: string;
		VSCODE_IPC_HOOK: string;
		VSCODE_CODE_CACHE_PATH: string;
		OPENCODE_DISABLE_DEFAULT_PLUGINS: string;
		FZF_CTRL_T_COMMAND: string;
		COMPOSER_NO_INTERACTION: string;
		CARGO_CFG_DEBUG_ASSERTIONS: string;
		npm_config_fund: string;
		FZF_DEFAULT_COMMAND: string;
		DEBIAN_FRONTEND: string;
		CARGO_PKG_VERSION: string;
		GOPATH: string;
		CARGO_CFG_PANIC: string;
		npm_config_user_agent: string;
		VSCODE_PID: string;
		CARGO_PKG_VERSION_MINOR: string;
		CARGO_CFG_TARGET_HAS_ATOMIC: string;
		CARGO_CFG_TARGET_FAMILY: string;
		INFOPATH: string;
		HOMEBREW_CELLAR: string;
		CARGO_MANIFEST_PATH: string;
		OSLogRateLimit: string;
		GIT_PAGER: string;
		VSCODE_CWD: string;
		RUSTC: string;
		DEBUG: string;
		CLAUDECODE: string;
		BAT_PAGER: string;
		npm_node_execpath: string;
		npm_config_prefix: string;
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
