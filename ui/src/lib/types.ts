export type LoaderKind = "vanilla" | "fabric" | "quilt" | "forge" | "neoforge";

export type Theme = "system" | "dark" | "light";

/** Palette layered over the dark/light theme. `spud` is the stock look. */
export type ThemePreset = "spud" | "midnight" | "forest" | "dusk" | "paper";

export type Density = "comfortable" | "compact";

export type Corners = "sharp" | "soft" | "round";

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
  ai_enabled: boolean;
  ai_provider: AiProvider;
  ai_endpoint: string;
  ai_model: string;
  ai_max_log_kb: number;
  sync_backend: SyncBackend;
  sync_folder: string | null;
  sync_keep: number;
  /** In-game overlay window. Off by default. */
  overlay_enabled: boolean;
  /** Global combination that toggles it, e.g. `Shift+Tab`. */
  overlay_hotkey: string;
  theme_preset: ThemePreset;
  density: Density;
  corners: Corners;
  /** File name inside `<root>/backgrounds/`; empty means none. */
  background: string;
  /** 0–100. */
  background_opacity: number;
  /** 0–40 px. */
  background_blur: number;
  /** 200–320 px. */
  sidebar_width: number;
  /** 960–1800 px. */
  content_width: number;
  reduce_motion: boolean;
}

export type AiProvider = "openai" | "anthropic";

/** Only `folder` ships today; the backend name is in the settings so adding
 * another (S3, Dropbox) does not need a schema change. */
export type SyncBackend = "folder";

/** What one instance's local sync bookkeeping says. */
export interface SyncInstanceStatus {
  instance_id: string;
  name: string;
  last_push_at: string | null;
  last_snapshot: string | null;
  last_pull_at: string | null;
  remote_updated_at: string | null;
  /** Another device pushed since this one last looked. */
  remote_newer: boolean;
}

export interface SyncStatus {
  configured: boolean;
  backend: string;
  folder: string | null;
  reachable: boolean;
  writable: boolean;
  keep: number;
  cache_dir: string;
  local: SyncInstanceStatus[];
}

export interface SyncSnapshot {
  file: string;
  sha256: string;
  bytes: number;
  created_at: string;
  origin: string;
  mc_version: string;
  loader: string;
  loader_version: string | null;
  note: string;
}

export interface SyncManifest {
  schema: number;
  instance_id: string;
  name: string;
  updated_at: string | null;
  snapshots: SyncSnapshot[];
}

export interface SyncRemoteView {
  instance_id: string;
  name: string;
  updated_at: string | null;
  snapshots: SyncSnapshot[];
}

export interface SyncPushResult {
  snapshot: SyncSnapshot;
  removed: string[];
  deduplicated: boolean;
  files: number;
  archive_bytes: number;
}

export interface SyncPullResult {
  snapshot: SyncSnapshot;
  instance: InstanceConfig;
  files: number;
}

/** What `overlay_status` reports about the shortcut, not about the window. */
export interface OverlayStatus {
  enabled: boolean;
  hotkey: string;
  registered: boolean;
  error: string | null;
  window: boolean;
  visible: boolean;
}

export interface LiveMetrics {
  /** `null` on the first sample and on platforms without CPU counters. */
  cpu_percent: number | null;
  memory_total_mb: number | null;
  memory_used_mb: number | null;
  process_rss_mb: number | null;
  process_id: number | null;
}

export interface OverlayInstance {
  id: string;
  name: string;
  mc_version: string;
  loader: string;
  loader_version: string | null;
  max_memory_mb: number;
  uptime_secs: number;
  pid: number;
}

export interface OverlayMod {
  title: string;
  version: string;
  file_name: string;
  bytes: number;
}

/** One image the launcher can paint behind the shell. */
export interface BackgroundFile {
  /** Bare file name — never a path. */
  name: string;
  mime: string;
  bytes: number;
}

export interface OverlayPayload {
  instance: OverlayInstance | null;
  metrics: LiveMetrics;
  mods: OverlayMod[];
  mods_total: number;
  captured_at: string;
}

/** What Settings → AI and the crash dialog need to know about the provider. */
export interface AiStatus {
  enabled: boolean;
  provider: AiProvider;
  endpoint: string;
  model: string;
  max_log_kb: number;
  /** Whether the OS keyring holds a key. The key itself never crosses the IPC. */
  has_key: boolean;
  /** False on systems without a credential store; the panel says so. */
  keyring: boolean;
}

export type Severity = "critical" | "warning" | "info";

export interface CrashDiagnosis {
  id: string;
  severity: Severity;
  title: string;
  evidence: string;
  fix: string[];
}

export interface RedactionReport {
  home_paths: number;
  tokens: number;
  uuids: number;
  emails: number;
}

/** Offline analysis: runs with no network, no key and no consent. */
export interface LocalCrashAnalysis {
  summary: string;
  exception: string | null;
  diagnoses: CrashDiagnosis[];
  mods: string[];
  truncated: boolean;
  bytes: number;
}

export interface CrashAnalysis {
  local: LocalCrashAnalysis;
  /** Redacted text the provider would receive. Present before consent. */
  preview: string | null;
  redaction: RedactionReport | null;
  ai: string | null;
  ai_error: string | null;
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
  tuning: TuningState | null;
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

export type GpuKind = "dedicated" | "integrated" | "unknown";

export interface HardwareProfile {
  platform: string;
  cpu_brand: string;
  cpu_cores: number;
  cpu_threads: number;
  memory_total_mb: number;
  memory_available_mb: number;
  gpu: string | null;
  gpu_kind: GpuKind;
}

export interface Tuning {
  min_memory_mb: number;
  max_memory_mb: number;
  jvm_args: string[];
  notes: string[];
}

export interface TuningPreview {
  instance_id: string;
  profile: HardwareProfile;
  tuning: Tuning;
  modded: boolean;
  java_major: number;
  current: boolean;
  applied_at: string | null;
  current_jvm_args: string[];
}

export interface TuningState {
  applied_at: string;
  cpu_threads: number;
  memory_total_mb: number;
  modded: boolean;
  java_major: number;
  max_memory_mb: number;
}

export interface StoreStats {
  blobs: number;
  bytes: number;
  index_path: string;
}

export interface DedupeReport {
  examined: number;
  reused: number;
  added: number;
  linked: number;
  copied: number;
  bytes_saved: number;
  problems: string[];
}

export interface GcReport {
  removed: number;
  bytes_freed: number;
  kept: number;
  dry_run: boolean;
  removed_hashes: string[];
}

export interface PluginEntry {
  id: string;
  name: string;
  version: string;
  api: number;
  description: string;
  author: string;
  main: string;
  hooks: string[];
  enabled: boolean;
  error: string | null;
  source: string | null;
  dir: string;
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

// ---------------------------------------------------------------------------
// Auto-update
// ---------------------------------------------------------------------------

export interface UpdateInfo {
  latest: string;
  url: string;
  available: boolean;
  notes: string;
}

// ---------------------------------------------------------------------------
// CurseForge
// ---------------------------------------------------------------------------

export interface CurseForgeHit {
  id: number;
  name: string;
  slug: string;
  summary: string;
  downloads: number;
  icon_url: string | null;
  author: string;
  game_versions: string[];
  mod_loaders: string[];
  date_modified: string;
  project_type: string;
  url: string;
}

export interface CurseForgeSearch {
  hits: CurseForgeHit[];
  total: number;
  page_size: number;
  index: number;
}

export interface CurseForgeSearchQuery {
  text: string;
  project_type: string;
  game_version?: string;
  loader?: string;
  limit: number;
  offset: number;
}
