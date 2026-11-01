export type LoaderKind = "vanilla" | "fabric" | "quilt" | "forge" | "neoforge";

export type Theme = "system" | "dark" | "light";

export interface LauncherSettings {
  schema: number;
  theme: Theme;
  locale: string;
  concurrency: number;
  default_memory_mb: number;
  default_java: string | null;
  close_on_launch: boolean;
  restore_on_exit: boolean;
  show_snapshots: boolean;
  mojang_meta: string;
  modrinth_api: string;
  curseforge_api_key: string | null;
  ms_client_id: string | null;
  auto_install_java: boolean;
  discord_rpc: boolean;
  discord_client_id: string | null;
}

export interface InstanceSummary {
  id: string;
  name: string;
  mc_version: string;
  loader: LoaderKind;
  loader_version: string | null;
  icon: string | null;
  last_played_at: string | null;
  max_memory_mb: number;
  path: string;
  ready: boolean;
}

export interface JavaSettings {
  mode: "auto" | "manual";
  path: string | null;
  min_memory_mb: number;
  max_memory_mb: number;
  extra_args: string[];
}

export interface WindowSettings {
  width: number;
  height: number;
  fullscreen: boolean;
}

export interface GameSpec {
  version: string;
  loader: LoaderKind;
  loader_version: string | null;
  version_id: string | null;
}

export interface InstanceConfig {
  schema: number;
  id: string;
  name: string;
  icon: string | null;
  created_at: string;
  updated_at: string;
  last_played_at: string | null;
  game: GameSpec;
  java: JavaSettings;
  window: WindowSettings;
  jvm_args: string[];
  game_args: string[];
  env: Record<string, string>;
  notes: string;
}

export interface NewInstance {
  name: string;
  mc_version: string;
  loader: LoaderKind;
  loader_version: string | null;
  max_memory_mb?: number;
}

export interface VersionSummary {
  id: string;
  kind: string;
  url: string;
  time: string;
  release_time: string;
  sha1: string | null;
}

export interface LoaderBuild {
  version: string;
  stable: boolean;
}

export interface JavaRuntime {
  path: string;
  major: number;
  version: string;
  vendor: string;
  arch: string;
}

export interface DeviceCode {
  device_code: string;
  user_code: string;
  verification_uri: string;
  expires_in: number;
  interval: number;
  message: string;
}

export interface WindowState {
  fullscreen: boolean;
  maximized: boolean;
  minimized: boolean;
}

export type AccountKind = "microsoft" | "offline";

export interface AccountView {
  id: string;
  username: string;
  uuid: string;
  active: boolean;
  kind: AccountKind;
}

export interface AccountsView {
  accounts: AccountView[];
  active: string | null;
}

export interface SearchHit {
  project_id: string;
  project_type: string;
  slug: string;
  author: string;
  title: string;
  description: string;
  categories: string[];
  display_categories: string[];
  versions: string[];
  downloads: number;
  follows: number;
  icon_url: string | null;
  date_modified: string;
  latest_version: string | null;
  license: string | null;
  client_side: string;
  server_side: string;
}

export interface ModrinthSearch {
  hits: SearchHit[];
  offset: number;
  limit: number;
  total_hits: number;
}

export interface SearchQuery {
  text: string;
  project_type: string;
  game_version?: string;
  loader?: string;
  limit: number;
  offset: number;
  index?: string;
}

export interface ModrinthProject {
  id: string;
  slug: string;
  title: string;
  description: string;
  body: string;
  project_type: string;
  categories: string[];
  client_side: string;
  server_side: string;
  downloads: number;
  followers: number;
  icon_url: string | null;
  source_url: string | null;
  issues_url: string | null;
  game_versions: string[];
  loaders: string[];
}

export interface VersionFile {
  hashes: Record<string, string>;
  url: string;
  filename: string;
  primary: boolean;
  size: number;
  file_type: string | null;
}

export interface VersionDependency {
  version_id: string | null;
  project_id: string | null;
  file_name: string | null;
  dependency_type: string;
}

export interface ModrinthVersion {
  id: string;
  project_id: string;
  name: string;
  version_number: string;
  version_type: string;
  game_versions: string[];
  loaders: string[];
  dependencies: VersionDependency[];
  files: VersionFile[];
  date_published: string;
  downloads: number;
}

export interface InstalledMod {
  project_id: string;
  version_id: string;
  project_title: string;
  version_number: string;
  file_name: string;
  sha1: string | null;
  size: number;
  project_type: string;
  installed_at: string;
}

export interface ModUpdate {
  project_id: string;
  title: string;
  installed_version: string;
  latest_version: string;
  latest_version_id: string;
}

export interface OptimizationResult {
  installed: InstalledMod[];
  skipped: string[];
}

export interface ModpackInfo {
  name: string;
  version_id: string;
  summary: string;
  minecraft: string;
  loader: LoaderKind;
  loader_version: string | null;
  file_count: number;
  download_bytes: number;
}

export interface ModpackInstallResult {
  instance: InstanceConfig;
  pack: ModpackInfo;
}

export interface InstallResult {
  version_id: string;
  files: number;
  bytes: number;
  instance: InstanceConfig;
}

export interface LaunchResult {
  instance_id: string;
  pid: number;
  log_file: string;
}

export interface RunningGame {
  instance_id: string;
  pid: number;
  started_at: string;
  log_file: string;
}

export interface StopResult {
  instance_id: string;
  pid: number | null;
}

export interface AuthStatus {
  configured: boolean;
  source: "env" | "settings" | "none";
  client_id: string | null;
}

export interface DownloadEvent {
  type: "progress" | "finished";
  completed?: number;
  total?: number;
  bytes?: number;
  current?: string;
  files?: number;
}

export interface JavaInstallResult {
  major: number;
  version: string;
  path: string;
  bytes: number;
}

export interface LogSnapshot {
  instance_id: string;
  path: string;
  exists: boolean;
  lines: string[];
}

export interface LogChunk {
  instance_id: string;
  lines: string[];
}

export interface LogFileInfo {
  name: string;
  path: string;
  bytes: number;
  modified: string | null;
}

export interface ExportResult {
  path: string;
  format: "zip" | "mrpack";
  files: number;
  bytes: number;
}

export interface ImportResult {
  instance: InstanceConfig;
  files: number;
}

export type DetectedLauncherKey =
  | "prism"
  | "multimc"
  | "polymc"
  | "modrinth"
  | "curseforge"
  | "atlauncher"
  | "gdlauncher"
  | "technic"
  | "lunar"
  | "vanilla";

export interface DetectedInstance {
  launcher: string;
  launcher_key: DetectedLauncherKey;
  name: string;
  path: string;
  mc_version: string | null;
  loader: LoaderKind;
  loader_version: string | null;
  mods: number;
  bytes: number;
}

export interface DetectedLauncher {
  key: DetectedLauncherKey;
  name: string;
  root: string;
  instances: number;
}

export interface ScanResult {
  launchers: DetectedLauncher[];
  instances: DetectedInstance[];
}

export interface ImportOutcome {
  instance: InstanceConfig;
  copied: number;
  skipped: number;
}

export interface FriendsConfig {
  nick: string;
  sync_folder: string | null;
  friends: string[];
}

export interface SharedInstance {
  id: string;
  name: string;
  mc_version: string;
  loader: string;
  loader_version: string | null;
  mods: number;
}

export interface SharedArchive {
  nick: string;
  instance: string;
  file_name: string;
  path: string;
  bytes: number;
}

export interface PresenceFile {
  nick: string;
  app: string;
  updated_at: string;
  status: string;
  server: string | null;
  instance: string | null;
  mc_version: string | null;
  loader: string | null;
  instances: SharedInstance[];
}

export interface FriendView {
  nick: string;
  online: boolean;
  status: string;
  server: string | null;
  instance: string | null;
  mc_version: string | null;
  loader: string | null;
  updated_at: string | null;
  instances: SharedInstance[];
  archives: SharedArchive[];
}
