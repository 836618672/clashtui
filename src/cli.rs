mod handler;
mod operations;
mod utils;
mod widgets;

pub use handler::handle_cli;

#[derive(clap::Parser)]
#[cfg_attr(debug_assertions, derive(Debug))]
#[command(
    version = utils::PKG_VERSION,
    long_version = utils::FULL_VERSION,
    about,
    after_help=concat!("If you have any question or suggestion, please visit ", env!("CARGO_PKG_REPOSITORY"))
)]
/// Mihomo (Clash.Meta) TUI Client
///
/// A tool for mihomo, also support other Clash API
pub struct Cmds {
    #[command(subcommand)]
    pub(crate) command: Option<ArgCommand>,
    #[arg(long, require_equals=true, num_args=0..=1, default_missing_value=None)]
    // `clashtui --generate-shell-completion` in fact get `Some(None)`
    // while `clashtui` get `None`
    /// generate shell completion
    generate_shell_completion: Option<Option<clap_complete::Shell>>,
    #[arg(long, require_equals = true)]
    /// specify the ClashTUI config directory
    pub config_dir: Option<std::path::PathBuf>,
    #[arg(long, short, action=clap::ArgAction::Count)]
    /// increase log level, default is Warning
    pub verbose: u8,
    #[cfg(feature = "customized-theme")]
    #[arg(long)]
    /// allow theme change without restart
    load_theme_realtime: bool,
}

/// Parse args, also handle envs(like `CLASHTUI_CONFIG_DIR`)
pub fn from_env() -> Cmds {
    use clap::Parser;
    let instance = Cmds::parse();

    Cmds {
        config_dir: instance
            .config_dir
            .or(std::env::var_os("CLASHTUI_CONFIG_DIR").map(std::path::PathBuf::from)),
        ..instance
    }
}

impl Cmds {
    /// `--generate_shell_completion` and `migrate`
    pub fn handle_early_exit(self) -> Option<Self> {
        if let Some(generate_shell_completion) = self.generate_shell_completion {
            utils::gen_complete(generate_shell_completion);
            eprint!("generate completion success");
            return None;
        }

        #[cfg(feature = "customized-theme")]
        if self.load_theme_realtime {
            crate::tui::Theme::enable_realtime();
        }

        Some(self)
    }
}

#[derive(clap::Subcommand)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub(crate) enum ArgCommand {
    /// Verify the bundled ClashTui dashboard (no download needed)
    Panel,
    /// Serve the bundled ClashTui core dashboard and local management
    Web {
        #[arg(long, default_value = "127.0.0.1:8080")]
        listen: std::net::SocketAddr,
        /// File containing a separate management token (minimum 24 characters)
        #[arg(long)]
        token_file: std::path::PathBuf,
    },
    /// Local workflows shared with TUI and Web; results are JSON
    Manage {
        #[arg(value_enum)]
        action: ManagementAction,
        #[arg(long)]
        name: Option<String>,
        #[arg(long, default_value = "profile", value_parser = ["profile", "template", "override"])]
        kind: String,
        /// Input file for import/save, settings patch or Provider groups
        #[arg(long)]
        input: Option<std::path::PathBuf>,
        #[arg(long)]
        url: Option<String>,
        #[arg(long)]
        new_name: Option<String>,
        #[arg(long)]
        template: Option<String>,
        /// Expected document revision for save (required to avoid lost edits)
        #[arg(long)]
        revision: Option<String>,
        #[arg(long)]
        with_proxy: Option<bool>,
        /// Confirm destructive or service-changing operations
        #[arg(long)]
        yes: bool,
    },
    /// Core API workflows shared with TUI; results are JSON
    Core {
        #[command(subcommand)]
        command: CoreCommand,
    },
    /// profile related
    Profile {
        #[command(subcommand)]
        command: ProfileCommand,
    },
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    /// service related
    Service {
        #[command(subcommand)]
        command: ServiceCommand,
    },
    /// set proxy mode,
    /// leave empty to get current mode
    Mode {
        /// Close existing connections after a successful mode change
        #[arg(long, global = true)]
        close_connections: bool,
        #[command(subcommand)]
        mode: Option<ModeCommand>,
    },
    /// check for update
    Update {
        /// check ci/alpha release instead
        #[arg(long, short = 'c')]
        ci: bool,
        /// target to check
        #[command(subcommand)]
        target: Target,
    },
}

#[derive(Debug, clap::Subcommand)]
pub(crate) enum Target {
    /// check for ClashTUI
    Clashtui,
    /// check for Mihomo
    Mihomo,
}

#[derive(clap::Subcommand)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub(crate) enum ModeCommand {
    /// rule
    Rule,
    /// direct
    Direct,
    /// global
    Global,
    /// Set a custom Mihomo mode
    Set { value: String },
}

#[derive(Clone, clap::ValueEnum)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub(crate) enum ProfileTypeFilter {
    /// file-based profiles
    File,
    /// URL-based profiles
    Url,
    /// template-based profiles
    Template,
}

impl ProfileTypeFilter {
    fn matches(&self, dtype: &crate::config::database::ProfileType) -> bool {
        matches!(
            (self, dtype),
            (
                ProfileTypeFilter::File,
                crate::config::database::ProfileType::File
            ) | (
                ProfileTypeFilter::Url,
                crate::config::database::ProfileType::Url(_)
            ) | (
                ProfileTypeFilter::Template,
                crate::config::database::ProfileType::Template { .. },
            )
        )
    }
}

#[derive(clap::Subcommand)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub(crate) enum ProfileCommand {
    /// update the selected profile or all
    Update {
        /// Update all profiles; updating an active profile also applies it
        #[arg(short, long, conflicts_with = "name")]
        all: bool,
        /// the profile name
        #[arg(short, long)]
        name: Option<String>,
        /// update profile with proxy
        #[arg(long, num_args=0..=1, default_missing_value="true", require_equals=true)]
        with_proxy: Option<bool>,
        /// update profile with proxyprovider removed
        #[arg(long)]
        without_proxyprovider: bool,
        /// filter by profile type
        #[arg(long, value_enum)]
        r#type: Option<ProfileTypeFilter>,
    },
    /// select profile
    Select {
        /// the profile name
        #[arg(short, long)]
        name: Option<String>,
    },
    /// list all profile
    List {
        /// without domain hint
        #[arg(long)]
        name_only: bool,
        /// filter by profile type
        #[arg(long, value_enum)]
        r#type: Option<ProfileTypeFilter>,
    },
}

#[derive(clap::Subcommand)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub(crate) enum ServiceCommand {
    /// Start the selected service
    Start,
    /// Read local service state
    Status,
    /// Stop the Mihomo service (compatibility alias)
    StopAll,
    #[cfg(windows)]
    Install,
    #[cfg(windows)]
    Uninstall,
    #[cfg(windows)]
    SystemProxy,
    /// start/restart service, can be soft
    Restart {
        /// restart by send POST request to mihomo
        #[arg(short, long)]
        soft: bool,
    },
    /// stop service
    Stop,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
#[value(rename_all = "snake_case")]
pub(crate) enum ManagementAction {
    State,
    Create,
    Import,
    Rename,
    Delete,
    Update,
    UpdateAll,
    Activate,
    NoPp,
    WithProxy,
    ProfileUrl,
    Traffic,
    Preview,
    Check,
    Test,
    Read,
    Save,
    Generate,
    PreviewTemplate,
    DeleteTemplate,
    TemplateProviders,
    SaveTemplateProviders,
    Runtime,
    Patch,
    Persist,
    PreparePanel,
    Start,
    Stop,
    Restart,
    StopAll,
}

impl ManagementAction {
    pub fn name(self) -> &'static str {
        // clap's possible-value spelling is the public API action spelling.
        match self {
            Self::State => "state",
            Self::Create => "create",
            Self::Import => "import",
            Self::Rename => "rename",
            Self::Delete => "delete",
            Self::Update => "update",
            Self::UpdateAll => "update_all",
            Self::Activate => "activate",
            Self::NoPp => "no_pp",
            Self::WithProxy => "with_proxy",
            Self::ProfileUrl => "profile_url",
            Self::Traffic => "traffic",
            Self::Preview => "preview",
            Self::Check => "check",
            Self::Test => "test",
            Self::Read => "read",
            Self::Save => "save",
            Self::Generate => "generate",
            Self::PreviewTemplate => "preview_template",
            Self::DeleteTemplate => "delete_template",
            Self::TemplateProviders => "template_providers",
            Self::SaveTemplateProviders => "save_template_providers",
            Self::Runtime => "runtime",
            Self::Patch => "patch",
            Self::Persist => "persist",
            Self::PreparePanel => "prepare_panel",
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
            Self::StopAll => "stop_all",
        }
    }
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub(crate) enum CoreResource {
    Rules,
    ProxyProviders,
    RuleProviders,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub(crate) enum ResourceAction {
    List,
    Update,
    UpdateAll,
    Health,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub(crate) enum MaintenanceAction {
    Restart,
    Upgrade,
    FlushDns,
    FlushFakeip,
    UpgradeGeo,
}

#[derive(clap::Subcommand)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub(crate) enum CoreCommand {
    Status,
    Proxies,
    /// Restore automatic selection for a pinned automatic group
    Unfix {
        group: String,
    },
    Select {
        group: String,
        node: String,
    },
    Delay {
        name: String,
        #[arg(long, conflicts_with = "group")]
        provider: Option<String>,
        #[arg(long)]
        group: bool,
        #[arg(long)]
        url: Option<String>,
        #[arg(long, default_value_t = 5000)]
        timeout: u64,
    },
    Resources {
        #[arg(value_enum)]
        kind: CoreResource,
        #[arg(value_enum, default_value = "list")]
        action: ResourceAction,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        yes: bool,
    },
    Connections {
        #[arg(long)]
        filter: Option<String>,
        #[arg(long)]
        close: bool,
        #[arg(long, conflicts_with = "filter")]
        id: Option<String>,
        #[arg(long)]
        yes: bool,
    },
    /// Finite authenticated log capture (JSON); --output never overwrites a file
    Logs {
        #[arg(long, default_value = "info")]
        level: String,
        #[arg(long)]
        filter: Option<String>,
        #[arg(long, default_value_t = 100)]
        limit: usize,
        #[arg(long, default_value_t = 10)]
        seconds: u64,
        #[arg(long)]
        output: Option<std::path::PathBuf>,
    },
    Metrics {
        #[arg(long, default_value_t = 1)]
        samples: usize,
        #[arg(long, default_value_t = 1000)]
        interval_ms: u64,
    },
    Maintenance {
        #[arg(value_enum)]
        action: MaintenanceAction,
        #[arg(long)]
        yes: bool,
    },
}

// use crate::backend::Mode;
// impl From<ModeCommand> for Mode {
//     fn from(value: ModeCommand) -> Self {
//         match value {
//             ModeCommand::Rule => Mode::Rule,
//             ModeCommand::Direct => Mode::Direct,
//             ModeCommand::Global => Mode::Global,
//         }
//     }
// }
