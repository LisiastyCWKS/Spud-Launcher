import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  AccountsView,
  AccountView,
  AuthStatus,
  DetectedInstance,
  DeviceCode,
  DownloadEvent,
  ExportResult,
  FriendView,
  FriendsConfig,
  ImportOutcome,
  ImportResult,
  InstallResult,
  InstanceConfig,
  InstanceSummary,
  InstalledMod,
  JavaInstallResult,
  JavaRuntime,
  LaunchResult,
  LauncherSettings,
  LoaderBuild,
  LoaderKind,
  LogChunk,
  LogFileInfo,
  LogSnapshot,
  ModrinthProject,
  ModrinthSearch,
  ModrinthVersion,
  ModUpdate,
  ModpackInstallResult,
  NewInstance,
  OptimizationResult,
  PresenceFile,
  RunningGame,
  ScanResult,
  SearchQuery,
  SharedArchive,
  StopResult,
  VersionSummary,
  WindowState,
} from "./types";

export const api = {
  instances: {
    list: () => invoke<InstanceSummary[]>("instances_list"),
    get: (id: string) => invoke<InstanceConfig>("instance_get", { id }),
    create: (draft: NewInstance) => invoke<InstanceConfig>("instance_create", { draft }),
    update: (config: InstanceConfig) => invoke<InstanceConfig>("instance_update", { config }),
    remove: (id: string) => invoke<void>("instance_delete", { id }),
    folder: (id: string) => invoke<string>("instance_folder", { id }),
    install: (id: string) => invoke<InstallResult>("instance_install", { id }),
    launch: (id: string) => invoke<LaunchResult>("instance_launch", { id }),
    running: () => invoke<RunningGame[]>("instances_running"),
    stop: (id: string) => invoke<StopResult>("instance_stop", { id }),
    requiredJava: (mcVersion: string) =>
      invoke<number>("instance_required_java", { mcVersion }),
    export: (id: string, format: "zip" | "mrpack") =>
      invoke<ExportResult>("instance_export", { id, format }),
    import: (path: string) => invoke<ImportResult>("instance_import", { path }),
  },
  logs: {
    snapshot: (id: string) => invoke<LogSnapshot>("instance_logs", { id }),
    files: (id: string) => invoke<LogFileInfo[]>("instance_log_files", { id }),
    read: (id: string, name: string) =>
      invoke<string>("instance_log_read", { id, name }),
    stream: (id: string) => invoke<boolean>("instance_log_stream", { id }),
  },
  launchers: {
    scan: () => invoke<ScanResult>("launchers_scan"),
    import: (instance: DetectedInstance, mcVersion: string) =>
      invoke<ImportOutcome>("launcher_import", {
        path: instance.path,
        name: instance.name,
        mcVersion,
        loader: instance.loader,
        loaderVersion: instance.loader_version,
      }),
  },
  friends: {
    get: () => invoke<FriendsConfig>("friends_get"),
    save: (config: FriendsConfig) =>
      invoke<FriendsConfig>("friends_save", { config }),
    add: (nick: string) => invoke<FriendsConfig>("friends_add", { nick }),
    remove: (nick: string) => invoke<FriendsConfig>("friends_remove", { nick }),
    list: () => invoke<FriendView[]>("friends_list"),
    archives: () => invoke<SharedArchive[]>("friends_archives"),
    publish: () => invoke<PresenceFile>("friends_publish"),
    share: (id: string) => invoke<SharedArchive>("friends_share_instance", { id }),
    import: (path: string) => invoke<ImportResult>("friends_import", { path }),
  },
  versions: {
    minecraft: (includeSnapshots: boolean) =>
      invoke<VersionSummary[]>("minecraft_versions", { includeSnapshots }),
    loaders: (loader: LoaderKind, mcVersion: string) =>
      invoke<LoaderBuild[]>("loader_versions", { loader, mcVersion }),
  },
  java: {
    detect: () => invoke<JavaRuntime[]>("java_detect"),
    inspect: (path: string) => invoke<JavaRuntime | null>("java_inspect", { path }),
    install: (major: number) => invoke<JavaInstallResult>("java_install", { major }),
  },
  settings: {
    get: () => invoke<LauncherSettings>("settings_get"),
    save: (settings: LauncherSettings) =>
      invoke<LauncherSettings>("settings_save", { settings }),
  },
  accounts: {
    list: () => invoke<AccountsView>("accounts_list"),
    status: () => invoke<AuthStatus>("auth_status"),
    beginLogin: () => invoke<DeviceCode>("account_begin_login"),
    finishLogin: (device: DeviceCode) =>
      invoke<AccountView>("account_finish_login", { device }),
    addOffline: (username: string) =>
      invoke<AccountsView>("account_add_offline", { username }),
    remove: (id: string) => invoke<AccountsView>("account_remove", { id }),
    setActive: (id: string) => invoke<AccountsView>("account_set_active", { id }),
  },
  modrinth: {
    search: (query: SearchQuery) => invoke<ModrinthSearch>("modrinth_search", { query }),
    project: (idOrSlug: string) =>
      invoke<ModrinthProject>("modrinth_project", { idOrSlug }),
    versions: (projectId: string, gameVersion?: string, loader?: string) =>
      invoke<ModrinthVersion[]>("modrinth_versions", {
        projectId,
        gameVersion: gameVersion ?? null,
        loader: loader ?? null,
      }),
  },
  mods: {
    installed: (instanceId: string) =>
      invoke<InstalledMod[]>("mods_installed", { instanceId }),
    install: (instanceId: string, projectId: string, versionId?: string) =>
      invoke<InstalledMod[]>("mods_install", {
        instanceId,
        projectId,
        versionId: versionId ?? null,
      }),
    remove: (instanceId: string, projectId: string) =>
      invoke<InstalledMod[]>("mods_remove", { instanceId, projectId }),
    updates: (instanceId: string) =>
      invoke<ModUpdate[]>("mods_check_updates", { instanceId }),
    installPack: (versionId: string) =>
      invoke<ModpackInstallResult>("modpack_install", { versionId }),
    optimize: (instanceId: string) =>
      invoke<OptimizationResult>("optimization_install", { instanceId }),
  },
  root: () => invoke<string>("data_root"),
  window: {
    state: () => invoke<WindowState>("window_state"),
    setFullscreen: (fullscreen: boolean) =>
      invoke<boolean>("window_fullscreen", { fullscreen }),
  },
};

type Unlisten = () => void;

export function onInstallProgress(
  handler: (event: DownloadEvent) => void,
  channel: "install://progress" | "mods://progress" = "install://progress",
): Promise<Unlisten> {
  return listen<DownloadEvent>(channel, (event) => handler(event.payload));
}

export function onGameLog(
  handler: (chunk: LogChunk) => void,
): Promise<Unlisten> {
  return listen<LogChunk>("game://log", (event) => handler(event.payload));
}

export function onGameChanged(handler: () => void): Promise<Unlisten> {
  return listen("game://changed", () => handler());
}
