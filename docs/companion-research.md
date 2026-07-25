# How Bitfocus Companion Works

## Scope

This document records what Companion's integration system actually is, at the
level of detail needed to make design decisions against it. It describes
Companion only. Nothing here is about launchpi, and nothing here is a
recommendation; `plugin-redesign.md` is where the conclusions live.

Everything below was read from `bitfocus/companion` at HEAD (Companion 5.x, July
2026), `bitfocus/companion-module-base` at HEAD (2.1.2) and at tag `v1.14.1`,
`bitfocus/companion-module-tools`, `bitfocus/companion-module-generic-osc`, and
`bitfocus/companion-module-template-ts`. Paths are repo-relative to whichever
repository is named.

## The Shape of the System

Companion is an Electron/Node application that owns a set of *surfaces*
(Stream Decks, Loupedecks, X-keys, remote satellites) and a set of *connections*
(configured instances of modules). Between them sit *controls* — buttons on
pages — which hold actions, feedbacks and a style.

An integration is a **module**: a Node.js package, distributed as a tarball,
that Companion launches as a **separate child process** and talks to over an
IPC channel. A module never touches Companion's memory, never draws to a
surface, and never sees a device. It receives configuration, publishes
definitions and variable values, answers feedback queries, and executes actions.

The pieces are named consistently throughout the codebase:

| Term | Meaning |
| --- | --- |
| Module | The package. `companion-module-bmd-atem`. |
| Connection | A configured instance of a module. Has a user-editable *label*. |
| Control | A button (or trigger) holding style, actions and feedbacks. |
| Entity | The umbrella for an action or feedback placed on a control. |
| Surface | A physical or remote panel. |
| Preset | A ready-made button definition a module ships. |

## Two API Lines Exist Simultaneously

This is the first thing to understand, because it changes every other answer.

| | **v1** — `@companion-module/base` 1.x | **v2** — `@companion-module/base` 2.x + `@companion-module/host` 1.x |
| --- | --- | --- |
| Who owns the IPC | `base` itself, in `src/host-api/api.ts` and `src/host-api/ipc-wrapper.ts` | **Companion**. `base` has no transport at all. |
| Child process entrypoint | The module's own file, which calls `runEntrypoint()` | Companion's own shim, `companion/lib/Instance/Connection/Thread/Entrypoint.ts`, which `import()`s the module's default export |
| Transport | Node `child_process` `'ipc'` channel | Length-framed messages over a unix domain socket |
| Encoding | **EJSON** (Meteor's extended JSON, `ejson@^2.2.3`) | Plain JSON |
| Protocol types live | In the public npm package | In Companion's private tree, typed against `@companion-app/shared/Model/*` |
| Handshake | `register { apiVersion, connectionId, verificationToken }` | `register { verificationToken }`, response carries `connectionId` and `moduleApiVersion` |
| `runEntrypoint` | Required | **Removed.** 2.0.0 breaking change: *"remove runEntrypoint method, expect default export instead"* |

Companion at HEAD runs both. `companion/package.json` pins them side by side:

```json
"@companion-module/base-old": "npm:@companion-module/base@~1.14.1",
"@companion-module/host": "1.1.1",
```

and `companion/lib/Instance/Connection/ApiVersions.ts` dispatches per module:

```ts
const range2_0_0OrLater = new semver.Range('>=2.0.0-0', { includePrerelease: true })
export function doesModuleUseNewChildHandler(apiVersion: SemVer | string): boolean {
	return range2_0_0OrLater.test(apiVersion)
}
```

selecting `ConnectionChildHandlerLegacy` or `ConnectionChildHandlerNew`.

Practically every module in the wild is still v1 — `generic-osc` pins
`"@companion-module/base": "1.13.6"`. But the direction of travel is that the
wire protocol becomes a Companion-internal detail and the stable public surface
becomes the TypeScript class API. That matters enormously to anyone considering
interop, and is revisited in `plugin-redesign.md`.

## Packaging and the Manifest

Modules are Node.js only. `companion-module-base/README.md` is explicit:

> In the future it may be possible to write modules in other languages, but it
> is not recommended… If you are interested in doing this then reach out and we
> can work together on creating an alternate framework.

A module repository is:

```text
companion/manifest.json     the only file Companion reads to identify the module
companion/HELP.md
package.json                main: entrypoint; dependency on @companion-module/base
src/main.ts | osc.js        the code
```

`yarn companion-module-build`, from `@companion-module/tools`, esbuild-bundles
the module to a single ESM file and rewrites the manifest
(`companion-module-tools/src/scripts/lib/build-util.ts`):

```ts
const esbuildOptions: esbuild.BuildOptions = {
	entryPoints, outdir: …, bundle: true, platform: 'node',
	format: 'esm', minify: isDev ? false : !buildConfig.disableMinifier,
	target: 'node22', external: externalsRaw, …
}
await esbuild.build(esbuildOptions)
await fs.copy('companion', path.join(packageBaseDir, 'companion'))

manifestJson.runtime.entrypoint = '../main.js'
manifestJson.version = srcPackageJson.version
manifestJson.runtime.api = 'nodejs-ipc'
manifestJson.runtime.apiVersion = frameworkPackageJson.version
```

That last line is the key fact about versioning: **`apiVersion` in a released
manifest is literally the version of `@companion-module/base` the module was
built against.** In development it is `"0.0.0"` and Companion resolves the real
version by reading the module's `node_modules`.

A real manifest, `companion-module-generic-osc/companion/manifest.json`:

```json
{
  "id": "generic-osc", "name": "generic-osc", "shortname": "osc",
  "description": "Direct: OSC plugin for Companion",
  "version": "0.0.0", "license": "MIT",
  "repository": "git+https://github.com/bitfocus/companion-module-generic-osc.git",
  "bugs": "…/issues",
  "maintainers": [{ "name": "William Viker", "email": "william@bitfocus.io" }],
  "legacyIds": ["direct_osc", "osc"],
  "runtime": {
    "type": "node22",
    "api": "nodejs-ipc",
    "apiVersion": "0.0.0",
    "entrypoint": "../osc.js"
  },
  "manufacturer": "Generic",
  "products": ["OSC"],
  "keywords": ["Protocol", "Generic"]
}
```

The schema is `companion-module-base@v1.14.1/assets/manifest.schema.json`.
Required: `id`, `name`, `shortname`, `description`, `version`, `license`,
`repository`, `bugs`, `maintainers`, `legacyIds`, `runtime`, `manufacturer`,
`products`, `keywords`. `runtime` requires `type`, `api`, `apiVersion`,
`entrypoint`. Optional: `runtime.permissions` (`worker-threads`,
`child-process`, `native-addons`, `filesystem`), `bonjourQueries`,
`isPrerelease`. v2 adds a required top-level `"type": "connection" | "surface"`.

`entrypoint` resolves as `path.join('companion', entrypoint)` relative to the
module directory — hence the `../` prefix — and is path-traversal checked.

**The manifest carries no capability schema.** It is identity and packaging
only. Actions, feedbacks, variables and presets are pushed at runtime; see
below. This is the single most consequential structural fact about the design.

## The Module Class API

### v1, which is what nearly every real module looks like

`companion-module-generic-osc/osc.js`, lightly trimmed:

```js
const { InstanceBase, Regex, runEntrypoint } = require('@companion-module/base');
const UpgradeScripts = require('./upgrades');

class OSCInstance extends InstanceBase {
	constructor(internal) { super(internal); }

	async init(config) {
		this.config = config;
		this.updateStatus('ok');
		this.updateActions();     // this.setActionDefinitions({ … })
		this.updateFeedbacks();   // this.setFeedbackDefinitions({ … })
		this.updateVariables();   // this.setVariableDefinitions([ … ])
	}
	async destroy() { this.log('debug', 'destroy'); }
	async configUpdated(config) { this.config = config; /* reconnect */ }

	getConfigFields() {
		return [
			{ type: 'textinput', id: 'host', label: 'Target Hostname or IP',
			  width: 8, regex: Regex.HOSTNAME, required: true },
			{ type: 'textinput', id: 'targetPort', label: 'Target Port',
			  width: 4, regex: Regex.PORT, required: true },
			{ type: 'checkbox', id: 'listen', label: 'Listen for Feedback',
			  width: 4, default: false, required: true },
			{ type: 'textinput', id: 'feedbackPort', label: 'Source Port', width: 4,
			  regex: Regex.PORT,
			  isVisible: (options, data) => options.listen && options.protocol === 'udp',
			  isVisibleExpression: "$(options:listen) === true && $(options:protocol) === 'udp'" },
		];
	}
}

runEntrypoint(OSCInstance, UpgradeScripts);
```

The abstract contract, `companion-module-base@v1.14.1/src/module-api/base.ts`:

```ts
abstract init(config: TConfig, isFirstInit: boolean, secrets: TSecrets): Promise<void>
abstract destroy(): Promise<void>
abstract configUpdated(config: TConfig, secrets: TSecrets): Promise<void>
abstract getConfigFields(): SomeCompanionConfigField[]
handleHttpRequest?(request: CompanionHTTPRequest): CompanionHTTPResponse | Promise<…>
handleStartStopRecordActions?(isRecording: boolean): void
```

Lifecycle calls are serialised through a `PQueue({ concurrency: 1 })` inside
`InstanceBase`, so `init`, `configUpdated` and `destroy` never overlap. The
consequence is that a module doing slow network I/O in `init` blocks its own
reconfiguration and its own teardown.

### v2

`companion-module-template-ts/src/main.ts`:

```ts
export type ModuleSchema = {
	config: ModuleConfig; secrets: undefined
	actions: ActionsSchema; feedbacks: FeedbacksSchema; variables: VariablesSchema
}
export { UpgradeScripts }
export default class ModuleInstance extends InstanceBase<ModuleSchema> {
	async init(config: ModuleConfig): Promise<void> { … }
	async destroy(): Promise<void> { … }
	async configUpdated(config: ModuleConfig): Promise<void> { … }
	getConfigFields(): SomeCompanionConfigField[] { return GetConfigFields() }
}
```

No `runEntrypoint`. The default export is the constructor, the named export
`UpgradeScripts` is the array. The whole class is generic over a `ModuleSchema`,
which gives end-to-end typing from option definitions through to callback
option values and variable ids.

### What `runEntrypoint` actually does

`companion-module-base@v1.14.1/src/entrypoint.ts`:

```ts
export function runEntrypoint<TConfig, TSecrets>(
	factory: InstanceConstructor<TConfig, TSecrets>,
	upgradeScripts: CompanionStaticUpgradeScript<TConfig, TSecrets>[],
): void {
	// 1. guard against being called twice
	// 2. read process.env.MODULE_MANIFEST, JSON.parse it
	if (manifestJson.runtime?.api !== HostApiNodeJsIpc) throw new Error(`Module manifest 'api' mismatch`)
	let apiVersion = manifestJson.runtime.apiVersion
	if (apiVersion === '0.0.0') { /* dev mode: read @companion-module/base's own package.json */ }
	if (!process.send) throw new Error('Module is not being run with ipc')

	const connectionId = process.env.CONNECTION_ID
	const verificationToken = process.env.VERIFICATION_TOKEN

	const ipcWrapper = new IpcWrapper<ModuleToHostEventsInit, HostToModuleEventsInit>(
		{}, (msg) => { process.send!(msg) }, 5000)
	process.once('message', (msg: any) => { ipcWrapper.receivedMessage(msg) })

	moduleInstance = new factory(literal<InstanceBaseProps<…>>({
		id: connectionId, upgradeScripts, _isInstanceBaseProps: true,
	}))

	ipcWrapper.sendWithCb('register', { apiVersion, connectionId, verificationToken })
		.then(() => console.log(`Module-host accepted registration`),
		      (err) => { console.error('Module registration failed', err); process.exit(11) })
}
```

Note the wart: the `InstanceBase` constructor installs its *own*
`process.on('message')` handler and its own `IpcWrapper`, while `runEntrypoint`
uses a second, throwaway `IpcWrapper` with `process.once('message')` purely for
the registration round-trip. Two wrappers share one channel. v2 collapsed this.

## The Process and Transport Model

### Spawning

`companion/lib/Instance/ProcessManager.ts`. Note `fork: false` plus an explicit
`'ipc'` stdio slot — Companion is not using `child_process.fork`, it is
constructing the IPC channel by hand:

```ts
const cmd: string[] = [
	nodePath,
	...getNodeJsPermissionArguments(moduleInfo.manifest, runtimeInfo.apiVersion, moduleInfo.basePath, enableInspect),
	...runtimeInfo.arguments,
	enableInspect ? `--inspect=${inspectPort}` : undefined,
	runtimeInfo.entrypoint,
].filter((v): v is string => !!v)

const monitor = new RespawnMonitor(cmd, {
	env: {
		...PreserveEnvVars(),
		VERIFICATION_TOKEN: child.authToken,          // nanoid()
		MODULE_MANIFEST: 'companion/manifest.json',
		...(dataChannel ? { MODULE_DATA_CHANNEL: dataChannel.socketPath } : {}),
		...runtimeInfo.env,                           // v1: { CONNECTION_ID }, v2: { MODULE_ENTRYPOINT }
	},
	maxRestarts: -1,
	kill: 5000,
	cwd: moduleInfo.basePath,
	fork: false,
	stdio: ['pipe', 'pipe', 'pipe', 'ipc'],
})
```

The node binary is a **bundled private runtime**, not the system node.
`companion/lib/Instance/NodePath.ts` resolves
`./node-runtimes/<platform>-<arch>-<ver>/bin/node` from
`assets/nodejs-versions.json`, keyed by `manifest.runtime.type`
(`node18` / `node22` / `node26`).

### Sandboxing

`getNodeJsPermissionArguments`, same file, applied only for `apiVersion >= 1.12.0`:

```ts
args.push('--no-warnings=SecurityWarning', '--permission',
	`--allow-fs-read=${moduleDir}`,
	`--allow-fs-read=${isPackaged() ? import.meta.dirname : path.join(import.meta.dirname, '../../..')}`)
if (manifest.runtime.type !== 'node22') args.push('--allow-net')
if (manifestPermissions['worker-threads']) args.push('--allow-worker')
if (manifestPermissions['child-process'] || manifestPermissions['native-addons']) args.push('--allow-child-process')
if (manifestPermissions['native-addons']) args.push('--allow-addons')
if (manifestPermissions['native-addons'] || manifestPermissions['filesystem'] || forceReadWriteAll)
	args.push('--allow-fs-read=*', '--allow-fs-write=*')
```

Node's permission model, driven by manifest-declared permissions. A module that
declares nothing gets read access to its own directory and nothing else.

### The v1 wire format

`companion-module-base@v1.14.1/src/host-api/ipc-wrapper.ts`. A hand-rolled
JSON-RPC-shaped protocol with numeric callback ids, where the payload is an
**EJSON string nested inside the outer object**:

```ts
export interface IpcCallMessagePacket {
	direction: 'call'
	name: string
	payload: string          // ejson.stringify(msg)
	callbackId: number | undefined
}
export interface IpcResponseMessagePacket {
	direction: 'response'
	callbackId: number
	success: boolean
	payload: string
}
```

`sendWithCb` allocates a `callbackId` and applies a default **5000 ms timeout**.
`sendWithNoCb` is fire-and-forget. Errors return `success: false` with
`JSON.stringify(err, Object.getOwnPropertyNames(err))` and are rehydrated into
an `Error` carrying the original `stack`.

EJSON is load-bearing, not incidental: it is Meteor's extended JSON, so
`Buffer`/`Uint8Array` become `{"$binary": "…"}` and `Date` becomes
`{"$date": n}`. Image buffers and shared-UDP payloads travel this way.

### Version negotiation

`ProcessManager.#findAndValidateModuleInfo`:

```ts
if (moduleInfo.manifest.runtime.api !== 'nodejs-ipc') return { error: 'Unsupported module runtime' }

let moduleApiVersion = moduleInfo.manifest.runtime.apiVersion
if (!moduleInfo.isPackaged) {
	// When not packaged, lookup the version from the library itself
	const moduleLibPackagePath = require.resolve('@companion-module/base/package.json', { paths: [moduleInfo.basePath] })
	moduleApiVersion = JSON.parse(await fs.readFile(moduleLibPackagePath, 'utf-8')).version
}
if (!isModuleApiVersionCompatible(moduleApiVersion)) return { error: 'Incompatible module version' }

if (doesModuleUseNewChildHandler(moduleApiVersion)) {
	return { apiVersion: moduleApiVersion,
	         entrypoint: path.join(import.meta.dirname, isPackaged() ? './ConnectionThread.js' : './Connection/Thread/Entrypoint.js'),
	         arguments: ['--enable-source-maps'],
	         moduleEntrypoint: jsFullPath,
	         env: { MODULE_ENTRYPOINT: jsFullPath },
	         usesDataChannel: true }
} else {
	return { apiVersion: moduleApiVersion,
	         entrypoint: jsFullPath, arguments: [],
	         moduleEntrypoint: jsFullPath,
	         env: { CONNECTION_ID: instanceId },
	         usesDataChannel: false }   // Legacy modules talk over the 'ipc' channel instead
}
```

The accepted range, `shared-lib/lib/ModuleApiVersionCheck.ts`:

```ts
export const MODULE_BASE_VERSIONS = [
	'1.14.0',
	'2.1.0',
	'2.1.2-nightly-main-20260722-105828-99d8e81', // DEV version
]
const moduleBaseRules = MODULE_BASE_VERSIONS.map((v) => {
	const parsedVersion = semver.parse(v)!
	return `${parsedVersion.major} - ${parsedVersion.major}.${parsedVersion.minor}.x`
})
const validModuleApiRange = new semver.Range(`~0.6 || ${moduleBaseRules.join(' || ')}`)
```

So `>=1.0.0 <=1.14.x` and `>=2.0.0 <=2.1.x`, plus a legacy `~0.6`. Beyond the
range check there are feature-flag predicates in `ApiVersions.ts`:
`doesModuleExpectLabelUpdates` (>=1.2), `doesModuleSupportPermissionsModel`
(>=1.12), `doesModuleUseSeparateUpgradeMethod` (>=1.13),
`doesModuleUseNewConfigLayout` (>=1.14).

### The v2 transport

`companion/lib/Instance/Connection/Thread/Entrypoint.ts` is what actually runs
as the child's `main`:

```ts
const moduleEntrypoint = process.env.MODULE_ENTRYPOINT
const manifestPath = process.env.MODULE_MANIFEST
const verificationToken = process.env.VERIFICATION_TOKEN
const dataChannelPath = process.env.MODULE_DATA_CHANNEL

// Everything the host passed us has now been captured, so take it back out of the environment. Module code
// runs in this process and has no business reading these.
delete process.env.MODULE_ENTRYPOINT; delete process.env.MODULE_MANIFEST
delete process.env.VERIFICATION_TOKEN; delete process.env.MODULE_DATA_CHANNEL

const dataSocket = await connectDataChannel(dataChannelPath)
dataSocket.on('close', () => process.exit())
…
const channel = new FramedChannel(dataSocket, (msg) => ipcWrapper.receivedMessage(msg as any))

ipcWrapper.sendWithCb('register', { verificationToken }).then(async (msg) => {
	const moduleImport = await importModuleFromPath(moduleEntrypoint)
	const moduleConstructor = typeof moduleImport === 'function' ? moduleImport : moduleImport.default
	if (typeof moduleConstructor !== 'function') throw new Error(`Module entrypoint did not return a valid constructor function`)
	const moduleUpgradeScripts = moduleImport.UpgradeScripts ?? []
	hostContext = new HostContext(ipcWrapper, msg.connectionId, moduleUpgradeScripts.length - 1)
	instance = new InstanceWrapper(msg.connectionId, hostContext, moduleConstructor, moduleUpgradeScripts, msg.moduleApiVersion)
	resolveInstanceReady()
}).catch((err) => { logger.error(…); process.exit(11) })
```

The v2 wrapper, `companion/lib/Instance/Common/IpcWrapper.ts`, is described in
its own comments as *"based upon the IpcWrapper from companion-module/base, with
ejson removed"*. Payloads are real JSON, and a third direction appears:

```ts
export interface IpcCancelMessagePacket {
	direction: 'cancel'
	callbackId: number
}
export type IpcMessagePacket = IpcCallMessagePacket | IpcResponseMessagePacket | IpcCancelMessagePacket
```

with `AbortSignal` plumbed into inbound handlers. The v2 message catalogue lives
in `companion/lib/Instance/Connection/IpcTypesNew.ts` — inside Companion — and
draws its types from Companion's private shared package:

```ts
import type { ClientEntityDefinition } from '@companion-app/shared/Model/EntityDefinitionModel.js'
import type { SomeCompanionInputField } from '@companion-app/shared/Model/Options.js'
import type { PresetDefinition, UIPresetSection } from '@companion-app/shared/Model/Presets.js'
```

## The v1 Message Catalogue

Verbatim from `companion-module-base@v1.14.1/src/host-api/api.ts`. The `=> never`
return type is the library's encoding for "fire-and-forget, no callback".

**Module → host** (`ModuleToHostEventsV0`):

```ts
'log-message': (msg: LogMessageMessage) => never
'set-status': (msg: SetStatusMessage) => never
setActionDefinitions: (msg: SetActionDefinitionsMessage) => never
setFeedbackDefinitions: (msg: SetFeedbackDefinitionsMessage) => never
setVariableDefinitions: (msg: SetVariableDefinitionsMessage) => never
setPresetDefinitions: (msg: SetPresetDefinitionsMessage) => never
setVariableValues: (msg: SetVariableValuesMessage) => never
updateFeedbackValues: (msg: UpdateFeedbackValuesMessage) => never
saveConfig: (msg: SaveConfigMessage) => never
'send-osc': (msg: SendOscMessage) => never
parseVariablesInString: (msg: ParseVariablesInStringMessage) => ParseVariablesInStringResponseMessage
upgradedItems: (msg: UpgradedDataResponseMessage) => void      // @deprecated since 1.13.0
recordAction: (msg: RecordActionMessage) => never
setCustomVariable: (msg: SetCustomVariableMessage) => never
// ModuleToHostEventsV0SharedSocket:
sharedUdpSocketJoin: (msg) => string
sharedUdpSocketLeave: (msg) => void
sharedUdpSocketSend: (msg) => void
```

**Host → module** (`HostToModuleEventsV0`):

```ts
init: (msg: InitMessage) => InitResponseMessage
destroy: (msg: Record<string, never>) => void
updateConfig: (config: unknown) => void                       // @deprecated, replaced 1.2.0
updateConfigAndLabel: (msg: UpdateConfigAndLabelMessage) => void
updateFeedbacks: (msg: UpdateFeedbackInstancesMessage) => void
updateActions: (msg: UpdateActionInstancesMessage) => void
upgradeActionsAndFeedbacks: (msg) => UpgradeActionAndFeedbackInstancesResponse   // since 1.13.0
executeAction: (msg: ExecuteActionMessage) => ExecuteActionResponseMessage | undefined  // response only since 1.14.0
getConfigFields: (msg) => GetConfigFieldsResponseMessage
handleHttpRequest: (msg) => HandleHttpRequestResponseMessage
learnAction: (msg) => LearnActionResponseMessage
learnFeedback: (msg) => LearnFeedbackResponseMessage
startStopRecordActions: (msg: StartStopRecordActionsMessage) => void
variablesChanged: (msg: VariablesChangedMessage) => never     // @deprecated since 1.13.0
sharedUdpSocketMessage / sharedUdpSocketError
```

Plus the pre-registration pair, `src/host-api/versions.ts`:

```ts
export const HostApiNodeJsIpc = 'nodejs-ipc'
export interface ModuleToHostEventsInit { register: (msg: ModuleRegisterMessage) => void }
export interface ModuleRegisterMessage { apiVersion: string; connectionId: string; verificationToken: string }
```

The v2 names are near-identical, plus `setCompositeElementDefinitions`, with
`upgradeActionsAndFeedbacks` split into `upgradeActions` and `upgradeFeedbacks`.

**Is this a public protocol?** No. `api.ts` carries this header:

> Warning: these types are intentionally semi-isolated from the module-api
> folder… it allows for us to be selective as to whether a change impacts the
> module api or the host api. This will allow for cleaner and more stable apis
> which can both evolve at different rates

It is versioned and type-defined, and for v1 it happens to be published on npm,
but there is no specification document and no compatibility promise. What
Bitfocus documents and promises is the TypeScript class API. The developer
portal is explicit:

> The `@companion-module/base` acts as a stable barrier between the two. It has
> intentionally been kept separate from the rest of the Companion code, so that
> changes made here get an extra level of scrutiny, as we want to guarantee
> backwards compatibility as much as possible.

## Actions

`companion-module-base@v1.14.1/src/module-api/action.ts`:

```ts
export interface CompanionActionDefinition {
	name: string
	description?: string
	options: SomeCompanionActionInputField[]
	/** Ignore changes to certain options and don't allow them to trigger the subscribe/unsubscribe callbacks */
	optionsToIgnoreForSubscribe?: string[]
	skipUnsubscribeOnOptionsChange?: boolean
	callback: (action: CompanionActionEvent, context: CompanionActionContext) => Promise<void> | void
	subscribe?: (action: CompanionActionInfo, context: CompanionActionContext) => Promise<void> | void
	unsubscribe?: (action: CompanionActionInfo, context: CompanionActionContext) => Promise<void> | void
	learn?: (action: CompanionActionEvent, context: CompanionActionContext)
		=> CompanionOptionValues | undefined | Promise<CompanionOptionValues | undefined>
	learnTimeout?: number
}
export interface CompanionActionInfo {
	readonly id: string; readonly controlId: string
	readonly actionId: string; readonly options: CompanionOptionValues
}
export interface CompanionActionEvent extends CompanionActionInfo {
	readonly surfaceId: string | undefined
}
```

`subscribe` and `unsubscribe` are driven from `src/internal/actions.ts`:
**every** `updateActions` message unsubscribes the old instance and subscribes
the new one, unless `skipUnsubscribeOnOptionsChange` is set. Any option edit
re-runs the pair. `optionsToIgnoreForSubscribe` narrows which option changes
count; v2 inverts it to `optionsToMonitorForSubscribe`.

Since 1.14 `callback` failures are returned rather than thrown:

```ts
} catch (e: any) {
	return { success: false, errorMessage: e?.message ?? String(e) }
}
```

v2 adds an `AbortSignal` on the context and lets `callback` return a `JsonValue`
result — `ExecuteActionSuccess { success: true; result: JsonValue | undefined }`
— which feeds an action result store.

`learn` is worth calling out because it has no analogue in most systems: given
an action's current options, the module queries the live device and returns
option values reflecting its present state. It is what powers the "learn" button
next to an action in the UI, and it is why configuring an ATEM transition in
Companion is a matter of setting the switcher up by hand and pressing one
button.

## Input Fields

`src/module-api/input.ts`:

```ts
export type InputValue = number | string | boolean | Array<string | number>
export interface CompanionOptionValues { [key: string]: InputValue | undefined }

export interface CompanionInputFieldBase {
	id: string
	type: 'static-text' | 'textinput' | 'dropdown' | 'multidropdown' | 'colorpicker'
	    | 'number' | 'checkbox' | 'custom-variable' | 'bonjour-device' | 'secret-text'
	label: string
	tooltip?: string
	description?: string
	/** @deprecated removed in 2.0.0. Use isVisibleExpression */
	isVisible?: (options: CompanionOptionValues, data: any | undefined) => boolean
	isVisibleExpression?: string
	/** @deprecated */ isVisibleData?: Record<string, any>
}
```

The concrete variants and their notable fields:

| Type | Fields |
| --- | --- |
| `static-text` | `value` |
| `textinput` | `default?`, `required?`, `regex?`, `useVariables?: boolean \| {local?}`, `multiline?` |
| `dropdown` | `choices: DropdownChoice[]`, `default`, `allowCustom?`, `regex?`, `minChoicesForSearch?` |
| `multidropdown` | as above plus `minSelection?`, `maxSelection?` |
| `colorpicker` | `default: string \| number`, `enableAlpha?`, `returnType?: 'string' \| 'number'`, `presetColors?` |
| `number` | `default`, `min`, `max`, `step?`, `range?`, `showMinAsNegativeInfinity?`, `showMaxAsPositiveInfinity?` |
| `checkbox` | `default: boolean` |
| `custom-variable` | picks a Companion custom variable |
| `bonjour-device` | config fields only; paired with `manifest.bonjourQueries` |
| `secret-text` | config fields only; stored in a separate secrets store |

Config fields additionally carry `width: number` on a 12-column grid.

### The `isVisible` wart

`src/internal/base.ts`:

```ts
export function serializeIsVisibleFn<T extends CompanionInputFieldBase>(options: T[]): EncodeIsVisible<T>[] {
	return (options ?? []).map((option) => {
		if ('isVisibleExpression' in option && typeof option.isVisibleExpression === 'string') {
			return { ...option, isVisibleFnType: 'expression', isVisibleFn: option.isVisibleExpression, … }
		} else if ('isVisible' in option && typeof option.isVisible === 'function') {
			return { ...option, isVisibleFn: option.isVisible.toString(), isVisibleFnType: 'function', … }
		}
		// ignore any existing `isVisibleFn` to avoid code injection
		return { ...option, isVisible: undefined, isVisibleFn: undefined, … }
	})
}
```

The module calls `Function.prototype.toString()` on a closure and ships the
**source text** across the IPC boundary; the host `eval`s it in the web UI to
decide field visibility. The comment shows they were aware of the hazard.
`isVisibleExpression` arrived in 1.12 using Companion's own expression language,
and the function form was deleted in 2.0.

## Feedbacks

`src/module-api/feedback.ts`. Three kinds:

```ts
export interface CompanionBooleanFeedbackDefinition extends CompanionFeedbackDefinitionBase {
	type: 'boolean'
	defaultStyle: Partial<CompanionFeedbackButtonStyleResult>
	callback: (feedback: CompanionFeedbackBooleanEvent, context: CompanionFeedbackContext) => boolean | Promise<boolean>
	/** If `undefined` or true, Companion will add an 'Inverted' checkbox … and handle the logic for you. */
	showInvert?: boolean
}
export interface CompanionValueFeedbackDefinition extends CompanionFeedbackDefinitionBase {
	type: 'value'
	callback: (…) => JsonValue | Promise<JsonValue>
}
export interface CompanionAdvancedFeedbackDefinition extends CompanionFeedbackDefinitionBase {
	type: 'advanced'
	callback: (feedback: CompanionFeedbackAdvancedEvent, context: CompanionFeedbackContext)
		=> CompanionAdvancedFeedbackResult | Promise<CompanionAdvancedFeedbackResult>
}
```

- **boolean** returns a bool. The definition carries `defaultStyle`, which the
  *host* applies when the feedback is true, and which the user may override per
  placement. The module never sees or computes the style.
- **value** returns arbitrary JSON, feeding local variables. Added in 1.13.
- **advanced** returns a whole style override, up to and including raw pixels:

```ts
export type CompanionFeedbackButtonStyleResult = Partial<CompanionButtonStyleProps>
export interface CompanionAdvancedFeedbackResult extends CompanionFeedbackButtonStyleResult {
	imageBuffer?: Uint8Array | string
	imageBufferEncoding?: { pixelFormat: 'RGB' | 'RGBA' | 'ARGB' }
	imageBufferPosition?: { x: number; y: number; width: number; height: number; drawScale?: number }
}
export interface CompanionButtonStyleProps {
	text: string; textExpression?: boolean
	size: 'auto' | '7' | '14' | '18' | '24' | '30' | '44' | number
	color: number; bgcolor: number
	alignment?: CompanionAlignment; pngalignment?: CompanionAlignment
	png64?: string; show_topbar?: boolean
}
```

Advanced feedbacks receive `image?: { width, height }` so they can size their
buffer to whatever surface is asking.

**Bitfocus now formally discourages advanced feedbacks.** The v2 doc comment:

> It is discouraged to use this type of feedback, as it does not fit into our
> graphics model, or user flexibility goals as well as the other types of
> feedback. This type will likely be removed in a future major version of the
> module API.

v2 also adds a required
`affectedProperties: Array<'text'|'size'|'color'|'bgcolor'|'alignment'|'pngalignment'|'png64'|'imageBuffer'> | undefined`
so the host can narrow invalidation to the graphics elements a feedback can
actually touch.

v1 has `subscribe`/`unsubscribe` on feedbacks. **v2 removed `subscribe` for
feedbacks** — the first `callback` invocation is the subscribe.

### The in-module recheck engine

`src/internal/feedback.ts`. This is the most carefully engineered part of the
module library:

```ts
#triggerCheckFeedback(id: string) {
	const existingRecheck = this.#feedbacksBeingChecked.get(id)
	if (existingRecheck) { existingRecheck.needsRecheck = true; return }   // collapse re-entrant checks
	…
	Promise.resolve().then(async () => {
		… value = definition.callback(event, context)
		const resolvedValue = await value
		this.#pendingFeedbackValues.set(id, { id, controlId: feedback.controlId, value: resolvedValue })
		this.#sendFeedbackValues()
	}).finally(() => {
		this.#feedbacksBeingChecked.delete(id)
		if (feedbackCheckStatus.needsRecheck) setImmediate(() => { this.#triggerCheckFeedback(id) })
	})
}

/** Send pending feedback values … with a debounce */
#sendFeedbackValues = debounceFn(() => {
	const newValues = this.#pendingFeedbackValues
	this.#pendingFeedbackValues = new Map()
	if (newValues.size > 0) this.#updateFeedbackValues({ values: Array.from(newValues.values()) })
}, { wait: 5, maxWait: 25 })
```

`checkFeedbacks(...types)` iterates every placed feedback instance of the
connection and triggers the matching ones. It is O(all placed feedbacks) per
call, with no coalescing at the call site; the re-entrancy collapse and the 5/25
ms debounce are the mitigations. v2 further added `AbortSignal` on the callback
context and an `abortable: boolean` starvation guard, because rechecks could
abort one another indefinitely.

Note the shape of the return path: the module reports `controlId` alongside each
value, so **the host never has to search for which control holds a feedback.**

## Variables

```ts
// v1
export interface CompanionVariableDefinition { variableId: string; name: string }
export type CompanionVariableValue = string | number | boolean
export interface CompanionVariableValues { [variableId: string]: CompanionVariableValue | undefined }
```

v2 changes `setVariableDefinitions` to take an object keyed by id, and widens
`CompanionVariableValue` to `JsonValue | undefined`.

**Namespacing is done entirely host-side, by connection label.** The module
sends bare ids; `companion/lib/Variables/Values.ts` qualifies them:

```ts
all_changed_variables_set.add(`${label}:${variable.id}`)
// Also report the old custom variable names as having changed
if (label === 'custom') all_changed_variables_set.add(`internal:custom_${variable.id}`)
```

Hence `$(mydeck:tally_1)`, where `mydeck` is the user's chosen connection label.
Renaming a connection renames every reference to it, and the
`internal:custom_x` alias above exists purely as a compatibility shim for an
earlier naming.

The module keeps a local mirror and refuses to publish values without
definitions, unless `instanceOptions.disableVariableValidation`:

```ts
} else if (this.#variableDefinitions.has(variableId)) {
	this.#variableValues.set(variableId, value ?? '')
	hostValues.push({ id: variableId, value: value ?? '' })
} else {
	// tell companion to delete the value
	hostValues.push({ id: variableId, value: undefined })
}
```

### Variables inside option fields

This changed direction mid-life and the history is instructive.

Originally a module called `context.parseVariablesInString(text)`, which was a
synchronous-looking call that was actually an IPC round trip:

```ts
parseVariablesInString: (msg: ParseVariablesInStringMessage) => ParseVariablesInStringResponseMessage
export interface ParseVariablesInStringResponseMessage { text: string; variableIds: string[] | undefined }
```

The returned `variableIds` were how Companion learned which variables the
action or feedback depended on — the dependency was inferred from what the
module happened to ask about.

**Since 1.13.0 this is inverted.** Companion parses option values *before*
sending them, and re-sends `updateActions`/`updateFeedbacks` when a referenced
variable changes. The method is deprecated:

> @deprecated Companion now handles this for you, for actions and feedbacks.

and in 2.0 it is deleted from both the class and the callback context. Variable
resolution now happens entirely in the host; modules receive plain values.

## Presets

`setPresetDefinitions(presets: CompanionPresetDefinitions)`. Each entry is
either:

```ts
CompanionButtonPresetDefinition {
	type: 'button', category: string, name: string,
	style: CompanionButtonStyleProps, previewStyle?: CompanionButtonStyleProps,
	options?: { relativeDelay?, stepAutoProgress?, rotaryActions? },
	feedbacks: CompanionPresetFeedback[],
	steps: CompanionButtonStepActions[],
}
```

or `CompanionTextPresetDefinition { type: 'text', category, name, text }`.

Presets are the reason Companion is usable rather than merely capable. A module
ships hundreds of ready-made buttons — "ATEM: Program 1", "OBS: Toggle Scene" —
complete with style, actions and the feedbacks that make them light up. The user
drags one onto a page and it works. Nothing else in the system does this job.

v2 overhauled them: `setPresetDefinitions(structure: CompanionPresetSection[], presets)`
splits the category tree from the definitions, adds layered graphics presets and
gauges, and lets presets embed *internal* actions and feedbacks alongside the
module's own.

## Configuration and Status

```ts
export type SomeCompanionConfigField = (…field union incl. BonjourDevice, Secret) & { width: number }
```

`getConfigFields()` is called on every edit. On first init, config is seeded from
each field's `default`, and `init` receives `isFirstInit: true`. 1.14 added
automatic layout on top of the manual `width` grid. Secrets go to a separate
store and are handed to `init`/`configUpdated` as a distinct `secrets` argument.

```ts
export enum InstanceStatus {
	Ok = 'ok', Connecting = 'connecting', Disconnected = 'disconnected',
	ConnectionFailure = 'connection_failure', BadConfig = 'bad_config',
	UnknownError = 'unknown_error', UnknownWarning = 'unknown_warning',
	AuthenticationFailure = 'authentication_failure',
}
updateStatus(status: InstanceStatus, message?: string | null): void
log(level: 'info' | 'warn' | 'error' | 'debug', message: string): void
```

v2 adds `InsufficientPermissions`. The distinction between `BadConfig`,
`AuthenticationFailure` and `ConnectionFailure` is not decorative — the UI
routes the user to a different remedy for each.

## Other Module Capabilities

- **Custom variables.** `context.setCustomVariableValue(name, value)` on the
  action context, over the `setCustomVariable` message. Marked
  `@deprecated Experimental: This method may change without notice. Do not use!`
  — it exists for a handful of internal modules. v2 replaced it with the action
  result store.
- **HTTP handlers.** `handleHttpRequest?(request: CompanionHTTPRequest): CompanionHTTPResponse`,
  where the request is an Express subset
  `{ baseUrl, body?, headers, hostname, ip, method, originalUrl, path, query }`
  and the response is `{ status?, headers?, body? }`. Whether a module has one
  is reported in `InitResponseMessage.hasHttpHandler`.
- **OSC.** `oscSend(host, port, path, args)` routes through Companion's shared
  OSC sender rather than the module opening its own socket.
- **Shared UDP.** `createSharedUdpSocket(type, cb)`. Companion owns the bind so
  several connections can receive on the same hardcoded port — the case where a
  protocol dictates the port and the user has two devices. Buffers cross the IPC
  as EJSON `$binary` in v1, base64 in v2.
- **Action recorder.** `handleStartStopRecordActions(isRecording)` plus
  `recordAction(action, uniquenessId?)`. The module watches its device and emits
  the actions that would reproduce what the user just did by hand.
- **Network helpers.** `TCPHelper`, `UDPHelper`, `TelnetHelper` in
  `src/helpers/`, re-exported from `@companion-module/base` — thin reconnecting
  wrappers over `node:net` and `node:dgram`, so modules do not each reimplement
  backoff.

**Connection modules have no surface-related API at all.** Surfaces are a
separate plugin type at HEAD — `companion/lib/Instance/Surface/IpcTypes.ts`,
with `@companion-surface/base` and `@companion-surface/host` packages, exposing
`drawControls`, `openHidDevice`, `scanDevices`, `setBrightness`, `setLocked`,
and inbound `input-press` / `input-rotate`. It is newer and even less public
than the connection API.

## The Render and Invalidation Pipeline

This is the part worth studying closely. There is no global tick and no render
frame. Invalidation is push-based through several debounce stages, with three
independent content-identity gates that kill redundant work outright.

### The cast

| Concern | Class | Path |
| --- | --- | --- |
| Variable store and change event | `VariablesValues` | `companion/lib/Variables/Values.ts` |
| Fan-out hub | `Registry` | `companion/lib/Registry.ts` |
| Control fan-out | `ControlsController` | `companion/lib/Controls/Controller.ts` |
| Per-button dependencies and debounce | `LayeredButtonDrawer` | `companion/lib/Controls/ControlTypes/Button/LayeredButtonDrawer.ts` |
| Per-element memo | `ElementConversionCache` | `companion/lib/Graphics/ElementConversionCache.ts` |
| Feedback pool | `ControlEntityListPoolBase` / `…Button` | `companion/lib/Controls/Entities/` |
| Render orchestration | `GraphicsController` | `companion/lib/Graphics/Controller.ts` |
| Rasterizer | `GraphicsRenderer`, `Image` | `companion/lib/Graphics/Renderer.ts`, `Image.ts` |
| Render handle | `ImageResult` | `companion/lib/Graphics/ImageResult.ts` |
| Surface delivery | `SurfaceHandler`, `SurfacePluginPanel` | `companion/lib/Surface/Handler.ts`, `PluginPanel.ts` |
| Web UI delivery | `PreviewGraphics` | `companion/lib/Preview/Graphics.ts` |
| Per-connection entity state | `ConnectionEntityManager` | `companion/lib/Instance/Connection/EntityManager.ts` |
| Internal module | `InternalController` | `companion/lib/Internal/Controller.ts` |

Names from older Companion versions that **no longer exist**, in case they turn
up in stale documentation: `SocketEventsHandler`, `VariablesInvalidateCache`,
`ControlBase.triggerRedraw()`, `GraphicsThread`, and socket.io.

### Flow A — a module publishes a variable

**1. Batch, module side.** `companion/lib/Instance/Connection/Thread/HostContext.ts`:

```ts
/**
 * Coalesce variable value updates before sending them over IPC, to avoid a flood of tiny messages
 * when a module pushes values very frequently (e.g. a stopwatch).
 */
readonly #variableValuesBatcher = new VariableValueBatcher<HostVariableValue>((values) =>
	this.#ipcWrapper.sendWithNoCb('setVariableValues', { newValues: values })
)
```

`VariableValueBatcher.ts` sets `VARIABLE_UPDATE_THROTTLE_MS = 20` and uses
`debounceFn({ wait: 20, maxWait: 20, before: true, after: true })` — leading edge
fires immediately, sustained bursts are capped near 50 Hz, and values are merged
latest-wins by id.

**2. Host handler.** `ChildHandlerNew.#handleSetVariableValues` calls
`this.#deps.variables.values.setVariableValues(this.#label, msg.newValues)`.

**3. Diff and emit.** `Variables/Values.ts` emits only genuinely-changed ids,
fully qualified. The emit is synchronous; there is no queue at this level.

```ts
if (moduleValues[variable.id] !== variable.value) {
	moduleValues[variable.id] = variable.value
	all_changed_variables_set.add(`${label}:${variable.id}`)
}
…
this.emit('variablesChanged', all_changed_variables_set, connection_labels, null)
```

**4. Fan-out.** `Registry.ts`:

```ts
#dispatchVariablesChanged(changedSet: ReadonlySet<string>, controlIdFilter: ReadonlySet<string> | null): void {
	this.internalModule.onVariablesChanged(changedSet, controlIdFilter)
	this.controls.onVariablesChanged(changedSet, controlIdFilter)
	this.instance.processManager.onVariablesChanged(changedSet, controlIdFilter)
	this.preview.onVariablesChanged(changedSet, controlIdFilter)
	// Surfaces only care about global changes, not control-scoped (local/page) variables
	if (controlIdFilter === null) this.surfaces.onVariablesChanged(changedSet)
}
```

**5. How does it know which buttons depend on which variables?**

**It does not keep a reverse index.** `ControlsController.onVariablesChanged`
iterates every control and asks each one:

```ts
for (const control of this.#store.controls.values()) {
	if (controlIdFilter && !controlIdFilter.has(control.controlId)) continue
	if (control.supportsEntities) control.entities.onVariablesChanged(allChangedVariablesSet)
	control.drawing?.onVariablesChanged(allChangedVariablesSet)
}
```

The rejection is cheap and lives on the control, using a set captured during the
**last render** and ES2024 `Set.prototype.isDisjointFrom`:

```ts
/** The variables referenced in the last draw. When one changes, a redraw is needed. */
#lastDrawVariables: ReadonlySet<string> | null = null
…
onVariablesChanged(allChangedVariables: ReadonlySet<string>): void {
	if (!this.#lastDrawVariables) return
	if (this.#lastDrawVariables.isDisjointFrom(allChangedVariables)) return
	this.elementConversionCache.queueInvalidateVariables(allChangedVariables)
	this.invalidate()
}
```

Rendering is what *discovers* the dependency set — `getDrawStyle()` returns
`usedVariables`, which becomes `#lastDrawVariables` for the next check. A render
is the subscription. The same pattern appears independently in
`ConnectionEntityManager` (`lastReferencedVariableIds`), `InternalController`
(`FeedbackEntityState.referencedVariables`), `ElementConversionCache` (per
element) and `PreviewSession`.

**6. Debounce.** The modern replacement for the old `triggerRedraw()`:

```ts
invalidate = debounceFn(() => {
	if (this.#pendingDraw) return
	this.#pendingDraw = true
	setImmediate(() => {
		this.deps.events.emit('invalidateControlRender', this.controlId)
		this.#pendingDraw = false
	})
}, { before: false, after: true, wait: 10, maxWait: 20 })
```

**7. Render queue.** `GraphicsController.invalidateControl` →
`invalidateButton(location)` → `#renderQueue.queue(id, …)`. That queue is an
`ImageWriteQueue` (`companion/lib/Resources/ImageWriteQueue.ts`) with
concurrency 5 and **key coalescing** — a render already queued for the same
button has its arguments replaced rather than a second job appended:

```ts
for (const img of this.#pendingImages) {
	if (img.key === key) { img.args = args; updated = true; break }
}
```

**8. Render and cache.** The style is computed lazily inside the queue callback,
then hashed into a content cache key:

```ts
const cacheKeyObj = { ...renderStyle, elements: collectContentHashes(buttonStyle.elements), referencedLocations: […].sort() }
const cacheKey = JSON.stringify(cacheKeyObj)
render = this.#renderLRUCache.get(cacheKey)
if (!render) { render = this.#generateImageResult(…); this.#renderLRUCache.set(cacheKey, render) }
…
const changed = this.#updateCacheWithRender(location, render)
skipInvalidation = skipInvalidation || !changed
if (!skipInvalidation) this.emit('button_drawn', location, render)
```

An identical render never emits. The LRU sizes itself between 100 and 1000
entries.

**9. Delivery.** Listeners on `button_drawn` are `SurfaceHandler`,
`PreviewGraphics`, `PreviewElementStream`, `LayeredButtonDrawer` (for
cross-button references), `CloudController`, `InternalControls`, and
`Registry → services.onButtonDrawn` for Satellite, EmberPlus and the HTTP API.

The unit of exchange is **`ImageResult`**, not a buffer. It is a lazy,
self-memoising, resolution-independent handle:

```ts
async drawNative(width, height, rotation: SurfaceRotation | null, format: imageRs.PixelFormat): Promise<Uint8Array>
async drawNativeEncoded(width, height, rotation, format: imageRs.ImageFormat): Promise<string>  // data: URL
readonly cacheKey: string | undefined   // content identity
readonly referencedLocations: ReadonlySet<string>
```

Each consumer asks for its own size, rotation and format, and results are
memoised per `${w}x${h}-${rot}-${fmt}`. One logical render simultaneously feeds
a 72 px Stream Deck, a 288 px web preview and a Satellite client, rasterising
once per distinct shape. Stream Decks go through `SurfacePluginPanel` →
`drawNative(w, h, rot, 'rgb')` → `buffer.toBase64()` → IPC `drawControls`, and
the plugin's queue *awaits* the child's acknowledgement, which is what gives
backpressure. The web UI goes `drawNativeEncoded(288, 288, null, 'png')` over a
tRPC subscription.

### Flow B — a module calls `checkFeedbacks`

1. The module recomputes, with the recheck collapse and 5/25 ms debounce
   described above, then sends `updateFeedbackValues`. This is **not** throttled
   host-side, unlike variables. `HostContext.updateFeedbackValues` base64s any
   `Uint8Array` `imageBuffer` on the way out.
2. `ChildHandlerNew.#handleUpdateFeedbackValues` calls
   `controls.updateFeedbackValues(connectionId, …)`. Because the module reported
   `controlId` with each value, the host does no searching.
3. `ControlStore.updateFeedbackValues` groups by `controlId` and hands each
   control its map.
4. `ControlEntityInstance.updateFeedbackValues` diffs with lodash `isEqual`
   against `#cachedFeedbackValue`. Unchanged values stop here.
5. Changed feedbacks map to the graphics elements they override:

```ts
for (const override of entity.styleOverrides) affectedElementIds.add(override.elementId)
this.reportChange({ redraw: true, noSave: true, changedElementIds: … })
```

6. That becomes `drawing.invalidateElement(id)` per element, then
   `triggerInvalidation()` — **the same 10/20 ms debounce as Flow A.** The two
   flows converge at `LayeredButtonDrawer.invalidate`.

### Internal feedbacks take the identical path

`InternalController.#checkFeedbacks` is the in-process analogue and lands in the
same sink, so internal and module feedbacks are indistinguishable downstream:

```ts
#checkFeedbacks(...types: string[]): void {
	const typesSet = new Set(types)
	const newValues: NewFeedbackValue[] = []
	for (const [id, feedback] of this.#feedbacks.entries()) {
		if (typesSet.size === 0 || typesSet.has(feedback.entityModel.definitionId)) {
			newValues.push({ entityId: id, controlId: feedback.controlId, value: this.#feedbackGetValue(feedback) })
		}
	}
	this.#controlsStore.updateFeedbackValues('internal', newValues)
}
```

### Summary of the timing stages

| Stage | Mechanism |
| --- | --- |
| Module variable batching | `debounceFn(wait 20, maxWait 20, before, after)` |
| Host variable diff | synchronous, only changed ids emitted |
| Fan-out to controls | synchronous, O(controls) with `isDisjointFrom` rejection |
| Feedback value diff | lodash `isEqual` against cached value |
| Per-button invalidation | `debounceFn(wait 10, maxWait 20)` + `setImmediate` |
| Render queue | `ImageWriteQueue`, concurrency 5, key-coalescing |
| Render dedup | LRU on JSON content key; `button_drawn` suppressed if unchanged |
| Full-page surface redraw | `debounceFn(wait 1, maxWait 5)` |

Three content-identity gates kill work with no timing involved at all: only
changed variable ids are emitted, feedback values are `isEqual`-diffed, and
`ImageResult.cacheKey` suppresses delivery of identical renders.

### Rasterisation

`GraphicsController` runs a `workerpool` of threads executing `@napi-rs/canvas`
(Skia) plus `@julusian/image-rs`:

```ts
#pool = workerPool.pool(path.join(import.meta.dirname, isPackaged() ? './RenderThread.js' : './Thread.js'), {
	minWorkers: 2,
	maxWorkers: Math.max(4, Math.floor(os.cpus().length * 0.67)),
	workerType: 'thread', …
})
```

with adaptive oversampling — 4× at Stream Deck sizes up to 96 px, 2× up to
168 px, 1× above — a pooled `Image` per `${w}x${h}x${oversample}`, a 5000-entry
text-layout LRU, and crash-loop protection that calls `process.exit(5)` if more
than 30 worker terminations occur within 60 seconds.

## Distribution, Install and Upgrade

`companion/lib/Instance/ModuleStore.ts` talks to the Bitfocus developer API for
the module list and version metadata. `InstalledModulesManager` downloads the
tarball and verifies it:

```ts
const bufferChecksum = crypto.createHash('sha256').update(fullTarBuffer).digest('hex')
if (bufferChecksum !== versionInfo.tarSha) return 'Download did not match checksum'
```

**A SHA-256 from the store API, and nothing else.** There is no code signing.
Modules install into `{moduleId}-{version}` directories; development modules
load from `--extra-module-path` or `COMPANION_DEV_MODULES`.

### Upgrade scripts

A module ships an ordered array of `CompanionStaticUpgradeScript` functions.
Companion persists a per-connection `upgradeIndex`, and on load sends
`upgradeActionsAndFeedbacks` with the stored index; the module runs every script
after that index over the stored config, actions and feedbacks, and returns the
migrated data plus the new index. This is how a module changes its option shape
without breaking every existing button. Before 1.13 the same job was done
through the now-deprecated `upgradedItems` message in the other direction.

## Failure Handling

`RespawnMonitor` restarts a crashed module with `maxRestarts: -1` and
exponential backoff `min(2^(i-3) * 1000, 60000)`. The connection's status
becomes `crashed`. Buttons keep whatever they last rendered.

There is no state checkpointing and no partial-degradation model: a crashed
connection silently freezes all of its variables and feedbacks at their last
values, and a button showing a stale tally looks exactly like a button showing a
live one.

Every `sendWithCb` carries a hard 5000 ms default timeout. A module callback that
blocks simply fails the call with `Call timed out` and no further diagnosis. v2
added `direction: 'cancel'` and `AbortSignal` specifically because there was
previously no way to cancel work already in flight.

## External Surfaces

Separately from the module API, Companion exposes a set of network services by
which a third party can drive it, read from it, or register itself as a panel.
These matter because they are the only route to Companion's integrations that
does not involve running its module code.

Everything below was read from `bitfocus/companion` at `main` (5.1.0-dev) and at
tag `v5.0.2`, plus `bitfocus/companion-satellite` and
`bitfocus/companion-surface-api`. Defaults live in
`companion/lib/Data/UserConfig.ts`; the model is
`shared-lib/lib/Model/UserConfigModel.ts`.

### Port and service map

| Service | Port | Default | Direction | Enable key | File |
| --- | --- | --- | --- | --- | --- |
| Web UI, HTTP API, tRPC WS | 8000 | on | in | `--admin-port` | `lib/UI/Express.ts`, `lib/UI/Handler.ts` |
| HTTPS | 8443 | off | in | `https_enabled` | `lib/Service/Https.ts` |
| **Satellite TCP** | **16622** | **on, not configurable** | in | none | `lib/Service/SatelliteTcp.ts` |
| **Satellite WebSocket** | **16623** | **on, not configurable** | in | none | `lib/Service/SatelliteWebsocket.ts` |
| TCP API | 16759 | off | in | `tcp_enabled` | `lib/Service/Tcp.ts` |
| UDP API | 16759 | off | in | `udp_enabled` | `lib/Service/Udp.ts` |
| OSC | 12321 | off | in | `osc_enabled` | `lib/Service/OscListener.ts` |
| Ember+ | 9092 | off | in | `emberplus_enabled` | `lib/Service/EmberPlus.ts` |
| Rosstalk | 7788 | off | in | `rosstalk_enabled` | `lib/Service/Rosstalk.ts` |
| Art-Net / DMX | 6454 | off | in | `artnet_enabled` | `lib/Service/Artnet.ts` |
| Prometheus | 8000 `/api/metrics` | off | in | `prometheus_enabled` + bearer | `lib/Data/Metrics.ts` |
| mDNS advertise | — | on | out | `mdns_announcements_enabled` | `lib/Service/MdnsAdvertise.ts` |
| Bitfocus Cloud | 443 → `api.bitfocus.io` | off | out | account login | `lib/Cloud/*.ts` |

Two services were **removed in 5.0** and became surface modules instead: the
Elgato Plugin websocket on 28492, and Videohub panel emulation. The migration is
in `lib/Data/Upgrades/v9tov10.ts`. `28492` appears nowhere in the 5.x tree.

### The Satellite protocol

This is the one genuinely designed-for-third-parties surface, and the only
supported way for a non-Node program to appear to Companion as a panel.

Implementation: `lib/Service/Satellite/SatelliteApi.ts` (protocol),
`SatelliteRenderUtil.ts` (bitmap encoding),
`SatelliteSurfaceManifestSchema.ts` (zod schema for the layout manifest),
`lib/Surface/IP/Satellite.ts` (per-surface send side). Two transports run the
identical `ServiceSatelliteApi`: raw TCP on 16622 (since Companion 2.2) and
WebSocket on 16623 (since 3.5, and it accepts **any** path).

`API_VERSION` is `1.13.0` on main and `1.12.0` on v5.0.2 — the delta is the
`leds` capability. There is no v2; it is one semver-versioned protocol. **No
specification file exists in either repository**; the normative source is the
changelog comment above `API_VERSION` in `SatelliteApi.ts`.

Framing is line-based on `\n` or `\r\n`, capped at 2 MB per line or the socket
is destroyed. The parser is `parseLineParameters()` in
`lib/Resources/Util.ts` — quote-aware, `\` escapes, split on the first `=`, bare
tokens becoming `true`. The serialiser emits booleans as `1`/`0`, numbers bare,
strings always quoted **with no escaping**, and a trailing space before the
newline.

On connect Companion immediately announces itself:

```text
BEGIN CompanionVersion="5.1.0+6xxx-main" ApiVersion="1.13.0" 
CAPS SUBSCRIPTIONS=0 NONSQUARE=1 BITMAP_FORMATS="rgb,png,webp" 
```

Client to server: `ADD-DEVICE`, `REMOVE-DEVICE`, `KEY-PRESS`, `KEY-ROTATE`,
`PINCODE-KEY`, `SET-VARIABLE-VALUE`, `CHANGE-PAGE`, `FIRMWARE-UPDATE-INFO`,
`ADD-SUB`, `REMOVE-SUB`, `SUB-PRESS`, `SUB-ROTATE`, `PING`, `PONG`, `QUIT`.
Server to client: `BEGIN`, `CAPS`, `KEY-STATE`, `KEYS-CLEAR`, `BRIGHTNESS`,
`VARIABLE-VALUE`, `LOCKED-STATE`, `DEVICE-CONFIG`, `SUB-STATE`, `PONG`, plus
`<CMD> OK|ERROR` acknowledgements.

Real wire lines:

```text
ADD-DEVICE DEVICEID="quickkeys:ABC123" LAYOUT_MANIFEST="eyJzdHlsZVBy..." PRODUCT_NAME="Quick Keys" VARIABLES="W10=" BRIGHTNESS=1 PINCODE_LOCK="FULL" SERIAL="ABC123" SERIAL_IS_UNIQUE=1 BITMAP_FORMAT="webp" 
ADD-DEVICE OK DEVICEID="quickkeys:ABC123" 
KEY-STATE DEVICEID="sd:XL1" CONTROLID="0/0" LOCATION="1/0/0" PRESSED=1 TYPE="BUTTON" BITMAP="data:image/webp;base64,UklGRi..." COLOR="#ff0000" TEXTCOLOR="#ffffff" TEXT="UGxheQ==" FONT_SIZE="auto" 
KEY-PRESS DEVICEID="sd:XL1" CONTROLID="0/3" KEY="0/3" PRESSED=1 
KEY-ROTATE DEVICEID="sd:XL1" CONTROLID="1/2" KEY="1/2" DIRECTION=1 
BRIGHTNESS DEVICEID="sd:XL1" VALUE=100 
LOCKED-STATE DEVICEID="sd:XL1" LOCKED=1 CHARACTER_COUNT=0 ROTATION=0 
VARIABLE-VALUE DEVICEID="qk:A1" VARIABLE="tbar" VALUE="NTA=" 
KEYS-CLEAR DEVICEID="sd:XL1" 
```

Four properties of this protocol are worth stating precisely.

**Bitmap format is negotiated, and resolution is client-chosen.** `BITMAP_FORMAT`
is one of `rgb`, `png`, `webp`, defaulting to `rgb` when absent or unrecognised
(`parseSatelliteBitmapFormat`). In `rgb` mode the `BITMAP` value is bare base64
of `w*h*3` bytes, row-major, already rotated by Companion. In `png`/`webp` mode
it is a self-describing data URL, and the `data:` prefix is the sole
discriminator. There is no fixed 72×72: the client declares `bitmap: {w, h}` per
style preset in its `LAYOUT_MANIFEST` and Companion renders at exactly that size
through `ImageResult.drawNative`, caching per shape. `CAPS NONSQUARE=1` signals
non-square support.

**Colour-and-text-only surfaces are first-class.** A style preset with no
`bitmap` and `colors: 'hex' | 'rgb'` receives `COLOR` and `TEXTCOLOR` and no
pixels at all; `text: true` adds base64 `TEXT`, `textStyle: true` adds
`FONT_SIZE` — which may be the literal string `"auto"`. Presets are assigned per
control, so a single surface can mix RGB-only pads, bitmap LCD keys and encoder
rings. The `leds` capability added in 1.13 describes an addressable ring as
`{segments: 24, mode: 'full-ring' | 'simple'}`, with `full-ring` defined as
segment 0 at six o'clock increasing clockwise. `LEDS` is always raw RGB base64
regardless of the negotiated bitmap format.

**Variables exist but are user-wired, not enumerable.** `ADD-DEVICE VARIABLES=`
carries base64 JSON declaring `{id, type: 'input' | 'output', name}`. An
`output` becomes an *expression* config field on the surface, which the user
fills in; Companion evaluates it and pushes `VARIABLE-VALUE` on change, debounced
5 ms with a 20 ms cap. An `input` becomes a custom-variable target that
`SET-VARIABLE-VALUE` writes to. You cannot ask for a module's variables — the
user wires each one up by hand in the UI.

**Subscriptions let you watch a button without being a surface.**
`ADD-SUB SUBID=x LOCATION=page/row/column [STYLE=… | BITMAP=72 COLORS=hex TEXT=1]`
yields `SUB-STATE` on every redraw and accepts `SUB-PRESS`/`SUB-ROTATE` upward.
This is gated behind `satellite_subscriptions_enabled`, which **defaults to
false**, and toggling it force-closes every satellite socket. The reference
client reads the capability flag, logs it, and does nothing with it.

Keepalive is asymmetric and aggressive: the reference client sends a bare `PING`
every 100 ms and disconnects after 15 unacknowledged pings with a second of
silence, while the server kills idle sockets at 5 s.

### HTTP API

`lib/Service/HttpApi.ts`, mounted at `/api` **with `cors()`** — deliberately
open cross-origin. Identical between 5.0.2 and main.

| Method | Path | R/W |
| --- | --- | --- |
| POST | `/api/location/:page/:row/:column/press \| down \| up \| rotate-left \| rotate-right` | W |
| POST | `/api/location/:page/:row/:column/step?step=N` | W |
| POST | `/api/location/:page/:row/:column/style` (`bgcolor`, `color`, `size`, `text`, `png64`, `alignment`) | W |
| POST / GET | `/api/custom-variable/:name/value` | W / R |
| GET | `/api/variable/:label/:name/value` | R |
| POST | `/api/surfaces/rescan` | W |
| GET | `/api/connections` → `{id, label, moduleId, enabled, status}[]` | R |
| GET | `/api/connections/:id/status` | R |
| POST | `/api/connections/:id/restart \| /enable \| /disable` | W |

Two answers matter more than the rest.

**A module variable can be read, one at a time, by name.**
`GET /api/variable/:label/:name/value` returns `text/plain`, 404 if unknown.
There is **no enumeration endpoint, no bulk dump, and no push**. Mirroring a
connection's variables over HTTP means discovering the names elsewhere and
polling N endpoints.

**A rendered button image cannot be read over HTTP at all.** There is no image
route in `HttpApi.ts`. Rendered images leave Companion over exactly three
channels: Satellite `BITMAP`, the tRPC preview subscriptions, and Bitfocus
Cloud. The `style` endpoint accepts `png64` as input and returns `'ok'`, with a
`// TODO - return style` in the source.

### tRPC over WebSocket

Companion's own web UI talks to the backend over **tRPC v11 on a WebSocket at
`/trpc`** on the admin port. `lib/UI/Handler.ts` creates it with
`new WebSocketServer({ noServer: true, path: '/trpc' })`. There is no HTTP tRPC
adapter mounted, so HTTP-batch tRPC 404s. **socket.io is entirely gone** from
5.x; the `socketcluster-client` dependency is the Cloud client and unrelated.

The wire format is friendly to a non-JS client: `initTRPC.create()` is called
with **no transformer**, so it is plain JSON over text frames. Server keepalive
is a WebSocket ping every 30 s with a 5 s pong deadline.

Root routers (`lib/UI/TRPC.ts`): `appInfo`, `bonjour`, `actionRecorder`,
`surfaces`, `controls`, `variables`, `customVariables`, `pages`, `importExport`,
`logs`, `userConfig`, `instances`, `cloud`, `usageStatistics`, `preview`,
`imageLibrary`.

The subscriptions that matter:

| Path | Payload |
| --- | --- |
| `preview.graphics.location {pageNumber,row,column}` | `{image: dataurl \| null, isUsed}` then one message per redraw |
| `preview.graphics.controlId` / `.preset` / `.reference` | data-url PNG |
| `surfaces.emulatorImages {id}` | bulk `{images: [{x,y,buffer}], clearCache}`, whole grid then deltas |
| `preview.expressionStream.watchExpression` | live expression or variable-string result, pushed |
| `variables.definitions.watch` | all variable *definitions* per label, plus deltas |
| `instances.connections.watch` | `Record<id, ClientConnectionConfig>` plus deltas |
| `instances.statuses.watch` | `{category, level, message}` per connection |
| `instances.definitions.actions` / `.feedbacks` / `.presets` | full definitions plus deltas |

Preview images here are **fixed 288×288 lossless PNG data URLs**
(`PREVIEW_RENDER_SIZE = 288`), not size-negotiable — unlike Satellite.

**Variable values have no subscription.** `variables.values.connection` is a
`.query()` returning `Record<name, value>` for one label; the web UI polls it at
1 Hz. For push you must open one `preview.expressionStream.watchExpression`
subscription per variable with `isVariableString: true`.

**There is no authentication.** Every procedure is `publicProcedure`;
`protectedProcedure` is commented out. `lib/UI/Handler.ts` says so directly:

> The tRPC api has no authentication, so without this any web page the user
> visits could open a WebSocket to a reachable Companion and drive the entire
> api.

The only gates are an `Origin` check and a loopback DNS-rebinding guard, and a
**missing** `Origin` header is explicitly allowed for non-browser tooling. A
client that omits `Origin` gets the full read-write API. `http_api_enabled` does
not gate `/trpc`. `admin_password` is compared client-side in
`webui/src/App.tsx` and is itself readable over `userConfig.watchConfig`, along
with `prometheus_token` and stored HTTPS private keys;
`instances.connections.watchEdit` returns per-connection module `secrets`.

The API is **internal, undocumented and unversioned** — zero occurrences of
"trpc" under `docs/` or in the changelog, typed structurally as
`export type AppRouter = ReturnType<typeof createTrpcRouter>`, with the web UI
importing it by relative path into the server source. The procedures listed
above happen to be unchanged between 5.0.2 and 5.1.0-dev; that is evidence, not
a promise.

### The other services

**OSC** on UDP 12321 mirrors the HTTP API's write half —
`/location/:page/:row/:column/{press,down,up,rotate-left,rotate-right,step}`,
style setters, `/custom-variable/:name/value`, `/surfaces/rescan`. It is
**write-only**; there is no reply channel.

**TCP and UDP on 16759** share `lib/Service/TcpUdpApi.ts` and take
newline-delimited text: `location P/R/C press`, `location P/R/C set-step N`,
`surface <id> page-up`, `custom-variable <n> set-value <v>` and
`custom-variable <n> get-value`. TCP replies `+OK`/`-ERR`; UDP does not reply.
TCP additionally pushes an unsolicited feed on every button redraw:

```json
{"type":"bank_bg_change","page":1,"row":0,"column":2,"red":255,"green":0,"blue":0}
```

That is the only push of button state outside Satellite and tRPC, and it is
**background colour only** — no text, no image.

**Ember+** on TCP 9092 is the most under-appreciated read surface. It exposes a
real tree: `0.2.<page>.<row>.<column>.{1,2,3,4}` gives `Pressed`, `Label`,
`Text_Color` and `Background_Color`, all ReadWrite so a client can both watch
and drive them; `0.3.1.<i>.1` and `0.3.2.<i>.1` expose internal and custom
variables with real types. It pushes on change. **But module variables are
excluded** — the change handler early-returns unless the label set contains
`internal` or `custom` — and adding a variable forces a debounced server restart
that drops every client.

**Art-Net** on UDP 6454 reads three DMX channels as page, bank and direction.
**Rosstalk** on TCP 7788 accepts `CC <page>:<bank>`. Both are write-only.

**Prometheus** at `/api/metrics` is the only authenticated surface anywhere: a
`nanoid(32)` bearer token compared with `timingSafeEqual`, 404 when disabled. It
exposes operational counters, not button or variable content.

**mDNS.** Since 5.0 Companion advertises two services so each SRV points at the
right port: `_companion-satellite-tcp._tcp` on 16622 and
`_companion-satellite-ws._tcp` on 16623. The TXT record carries `id`, `version`
and — usefully — **`protocolVersion`, the Satellite `API_VERSION`**, so a client
can screen for compatibility before connecting. Re-announced every 60 s. In the
other direction `lib/Surface/Discovery.ts` browses `_companion-satellite._tcp`,
which is Satellite advertising its own REST port 9999, and Companion POSTs
`{host, port: 16622}` to `http://<sat>:9999/api/config` to point it at itself.

### What can actually be read out

| Want | Satellite | HTTP | TCP/UDP | Ember+ | tRPC WS |
| --- | --- | --- | --- | --- | --- |
| Connection list and status | ✗ | ✓ poll | ✗ | ✗ | ✓ push |
| Action / feedback definitions | ✗ | ✗ | ✗ | ✗ | **✓ push, only here** |
| Every module variable's value | ~ user-wired expressions only | ~ one at a time by name, no enumeration | ~ custom variables only | ~ internal and custom only | ✓ names pushed, values polled or one subscription each |
| Rendered image of every button | **✓ push, size negotiable** | ✗ | ~ background colour only | ~ colours and label | ✓ push, fixed 288 px |
| Preset list | ✗ | ✗ | ✗ | ✗ | **✓ push, only here** |

Stated bluntly: **action definitions, feedback definitions and presets are
obtainable only over the undocumented tRPC WebSocket.** Rendered images are best
obtained over Satellite, which is purpose-built and lets the consumer choose the
resolution. A complete live mirror of module variables has no good answer
anywhere in the system.

### Registering as a surface

Four mechanisms exist. Only one is usable by a non-Node program.

1. **Satellite** — versioned, documented, transport- and language-agnostic. The
   surface appears in Companion's Surfaces table like local hardware and gets
   brightness, lock, pincode, page-change and user-editable config fields.
2. **`@companion-surface/base`** — `bitfocus/companion-surface-api`. A
   TypeScript in-process interface; `engines` is `node ^22.21 || ^26.5` and the
   manifest's only legal `runtime.type` values are `node22` and `node26`.
   Companion loads these in-process; Satellite spawns them as Node child
   processes over a bespoke, undocumented, unversioned IPC. **Not reachable from
   a non-Node host** without shipping a Node shim, at which point one is
   reimplementing Satellite.
3. **Elgato Plugin protocol on 28492** — removed in 5.0.
4. **Videohub panel emulation** — now a surface module, and a Blackmagic control
   protocol rather than a general surface API.

### Security posture

There is effectively none, and the project says so. `docs/user-guide/security.md`:

> Although none of these features makes an installation secure, they can help to
> stop casual browsers.

The HTTP API has no token and is on by default with permissive CORS. Satellite
has no authentication, no TLS, no allowlist, and **no way to disable it or move
its port**. The tRPC WebSocket has no authentication and leaks `admin_password`,
`prometheus_token`, HTTPS private keys and per-connection module secrets. TCP,
UDP, OSC, Ember+, Rosstalk and Art-Net have no auth but are all off by default.
Prometheus is the sole authenticated surface. `admin_lockout` is a client-side
check and not a control. Only two operations are genuinely restricted to local
clients: custom module bundle import and udev rule application.

A reachable Companion should be treated as fully controllable by anyone on the
network.

## Known Pain Points

These are the design problems visible in the code and, in several cases,
acknowledged by Bitfocus in doc comments or changelogs.

1. **String-typed, label-namespaced variables.** `$(label:name)` where `label`
   is the user's editable connection name. Renaming a connection invalidates
   every reference to it, and `internal:custom_x` is emitted as a permanent
   alias to paper over an earlier rename.
2. **Executable source over the wire.** `isVisible.toString()` shipped to the
   host to be `eval`'d. Removed in 2.0, but it was live for years.
3. **Advanced feedbacks.** Letting a module return an arbitrary style blob, or
   raw pixels, defeats render caching, defeats user style overrides, and defeats
   layered graphics. Now documented as discouraged and slated for removal;
   `affectedProperties` in v2 is a retrofit for a problem a narrower API would
   never have had.
4. **Variable resolution inside the plugin.** `parseVariablesInString` was an
   IPC round trip that looked like a function call, invoked from inside feedback
   callbacks, with the host inferring dependencies from the return value.
   Deprecated in 1.13, deleted in 2.0.
5. **Unbounded `checkFeedbacks`.** An O(all placed feedbacks) sweep with no
   coalescing at the call site. The recheck collapse, the 5/25 ms debounce, the
   `AbortSignal` and the `abortable` starvation guard are all mitigations bolted
   on afterwards.
6. **Fixed-timeout fire-and-forget calls.** 5000 ms on everything, with no
   cancellation until v2.
7. **Two `IpcWrapper`s sharing one channel** in v1, a symptom of the
   entrypoint/instance split. Collapsed in v2.
8. **The 1.x → 2.x migration is enormous.** The 2.0.0-alpha changelog lists
   fifteen breaking changes at once: ESM only, `setVariableDefinitions`
   array → object, `runEntrypoint` removed, `parseVariablesInString` removed,
   preset restructure, `subscribe` removed for feedbacks,
   `optionsToIgnoreForSubscribe` → `optionsToMonitorForSubscribe`,
   `InputValue` → `JsonValue`, `relativeDelay` removed, learn semantics changed,
   `checkAllFeedbacks` split out, and more. Every module must be hand-ported,
   and the previous such migration is still not finished across the ecosystem.
9. **Blocking `init`.** Serialised behind a `concurrency: 1` queue, so a module
   doing network I/O in `init` blocks its own `configUpdated` and `destroy`.
10. **Respawn is the entire crash story.** No checkpoint, no staleness marking,
    no way for a button to say "this value is from a dead connection".
11. **The rich read surface is the undocumented one.** Everything a third party
    would actually want — action and feedback definitions, presets, variable
    definitions, per-button renders — is available only over an internal,
    unversioned tRPC WebSocket with no authentication, while the documented
    HTTP API can read exactly one variable at a time by name and no images at
    all.
12. **No authentication anywhere that matters.** Satellite cannot be disabled or
    moved off 16622, tRPC leaks `admin_password` and per-connection module
    secrets to any client that omits an `Origin` header, and `admin_lockout` is
    a client-side string comparison.
13. **Two protocol families for one job.** Satellite exists to let a foreign
    panel act as a surface; `@companion-surface/base` exists to let a Node
    package act as a surface. They overlap heavily, and Satellite itself has to
    host the latter as child processes over yet another bespoke IPC.

## What Companion Gets Right

Stated plainly, because these are the parts worth learning from.

1. **Three independent content-identity gates.** Changed-ids-only variable
   emission, `isEqual` on feedback values, and `cacheKey` on renders. Any one of
   them alone would be insufficient; together they make feedback storms
   survivable.
2. **Dependency sets discovered by rendering.** No reverse index to keep
   coherent, no class of bugs where the index disagrees with reality, and the
   check is a single `isDisjointFrom`.
3. **Lazy, resolution-independent render handles.** `ImageResult` does not
   rasterise until a consumer names a size, rotation and format, then memoises
   per shape. One render feeds every consumer at its own resolution.
4. **The host never searches on the return path.** Feedback values carry their
   `controlId`.
5. **Debouncing at every layer boundary, always with `maxWait`.** Latency stays
   bounded no matter how chatty the source.
6. **A separately versioned API package with its own release cadence.** The
   "stable barrier" idea is sound even though the wire format underneath it is
   not public.
7. **Presets.** The single largest contributor to Companion being usable rather
   than merely powerful.
8. **Definitions are runtime data, not manifest data.** A module that has
   connected to an ATEM knows how many inputs it has, and can offer them as
   dropdown choices. A static manifest could never express that.

## References

- [bitfocus/companion](https://github.com/bitfocus/companion)
- [bitfocus/companion-module-base](https://github.com/bitfocus/companion-module-base)
- [bitfocus/companion-module-tools](https://github.com/bitfocus/companion-module-tools)
- [bitfocus/companion-module-generic-osc](https://github.com/bitfocus/companion-module-generic-osc)
- [bitfocus/companion-module-template-ts](https://github.com/bitfocus/companion-module-template-ts)
- [bitfocus/companion-satellite](https://github.com/bitfocus/companion-satellite)
- [bitfocus/companion-surface-api](https://github.com/bitfocus/companion-surface-api)
- [The Companion Module Libraries](https://companion.free/for-developers/module-development/module-lifecycle/companion-module-library/)
- [Module API Changelog](https://companion.free/for-developers/module-development/api-changes/)
- [Module packaging](https://companion.free/for-developers/module-development/module-lifecycle/module-packaging)
- [Satellite API](https://companion.free/for-developers/Satellite-API/)
- [Surface Developers' Guide](https://companion.free/for-developers/surface-development/)
