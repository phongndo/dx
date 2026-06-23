#[cfg(not(test))]
use std::process::{Command, Stdio};
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    env, fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    ARTIFACT_SOURCE, ASM_HIGHLIGHTS_QUERY, BASENAME_LANGUAGES, COMMONLISP_HIGHLIGHTS_QUERY,
    CORE_LANGUAGES, CUSTOM_PARSER_SOURCE, CUSTOM_PARSER_VERSION, DiffContextExpansion,
    DiffSettings, HIGHLIGHT_NAMES, LANGUAGE_ALIASES, LANGUAGE_PACK_VERSION, OCAML_HIGHLIGHTS_QUERY,
    StoredDiffContextExpansion, StoredDiffContextExpansionMode, StoredDiffSettings,
    StoredLanguageMapping, StoredParserArtifact, StoredSyntaxConfig, StoredSyntaxLimits,
    StoredSyntaxSettings, StoredSyntaxThemeConfig, StoredSyntaxThemeTable, SyntaxLimits,
    SyntaxMode, SyntaxSettings, SyntaxThemeConfig, SyntaxThemeSource, TRUSTED_PARSER_MANIFEST,
    TRUSTED_PARSER_MANIFEST_SHA256, cache_dir, config_path, load_settings, parsers_dir,
    queries_dir,
};
use mark_core::{MarkError, MarkResult};
use sha2::{Digest, Sha256};
use tree_sitter_highlight::HighlightConfiguration;
use tree_sitter_language_pack::LanguageRegistry;

const VALIDATION_CHILD_ENV: &str = "MARK_SYNTAX_VALIDATION_CHILD";
const VALIDATION_LANGUAGE_ENV: &str = "MARK_SYNTAX_VALIDATION_LANGUAGE";
const VALIDATION_PARSER_ENV: &str = "MARK_SYNTAX_VALIDATION_PARSER";
const VALIDATION_QUERY_ENV: &str = "MARK_SYNTAX_VALIDATION_QUERY";
const VALIDATION_CHILD_SUCCESS: &[u8] = b"mark-syntax-validation-ok\n";

pub(crate) fn config_home() -> MarkResult<PathBuf> {
    if let Some(path) = env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path));
    }

    #[cfg(windows)]
    {
        if let Some(path) = env::var_os("APPDATA").filter(|value| !value.is_empty()) {
            return Ok(PathBuf::from(path));
        }
        if let Some(path) = env::var_os("USERPROFILE").filter(|value| !value.is_empty()) {
            return Ok(PathBuf::from(path).join("AppData").join("Roaming"));
        }
    }

    env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .map(|home| home.join(".config"))
        .ok_or_else(|| MarkError::Usage("could not determine config directory".to_owned()))
}

pub(crate) fn load_config() -> MarkResult<StoredSyntaxConfig> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(StoredSyntaxConfig::default());
    }

    let contents = fs::read_to_string(&path)?;
    serde_json::from_str(&contents).map_err(Into::into)
}

pub(crate) fn save_config(config: &StoredSyntaxConfig) -> MarkResult<()> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let contents = serde_json::to_vec_pretty(config)?;
    fs::write(path, contents)?;
    Ok(())
}

pub(crate) fn parse_settings(contents: &str) -> Result<SyntaxSettings, toml::de::Error> {
    let stored: StoredSyntaxSettings = toml::from_str(contents)?;
    Ok(settings_from_stored(stored))
}

pub(crate) fn settings_from_stored(stored: StoredSyntaxSettings) -> SyntaxSettings {
    let colorscheme = stored.colorscheme.or(stored.theme);

    SyntaxSettings {
        mode: stored.mode.unwrap_or_default(),
        theme: colorscheme
            .map(theme_config_from_stored)
            .unwrap_or_default(),
        colors: stored.colors.overlay(stored.color_overrides),
        transparent_background: stored.transparent_background,
        diff: diff_from_stored(stored.diff),
        limits: limits_from_stored(stored.limits),
    }
}

pub(crate) fn diff_from_stored(stored: StoredDiffSettings) -> DiffSettings {
    let defaults = DiffSettings::default();
    DiffSettings {
        line_background: stored.line_background.unwrap_or(defaults.line_background),
        gutter_background: stored
            .gutter_background
            .unwrap_or(defaults.gutter_background),
        inline_background: stored
            .inline_background
            .or(stored.word_background)
            .unwrap_or(defaults.inline_background),
        sign_style: stored.sign_style.unwrap_or(defaults.sign_style),
        context_expansion: stored
            .context_expansion
            .map(diff_context_expansion_from_stored)
            .unwrap_or(defaults.context_expansion),
    }
}

pub(crate) fn diff_context_expansion_from_stored(
    stored: StoredDiffContextExpansion,
) -> DiffContextExpansion {
    match stored {
        StoredDiffContextExpansion::Lines(lines) => DiffContextExpansion::Lines(lines.max(1)),
        StoredDiffContextExpansion::Mode(StoredDiffContextExpansionMode::Full) => {
            DiffContextExpansion::Full
        }
    }
}

pub(crate) fn theme_config_from_stored(stored: StoredSyntaxThemeConfig) -> SyntaxThemeConfig {
    match stored {
        StoredSyntaxThemeConfig::Name(name) => theme_config_from_name(name),
        StoredSyntaxThemeConfig::Table(table) => theme_config_from_table(table),
    }
}

pub(crate) fn theme_config_from_name(name: String) -> SyntaxThemeConfig {
    let name = name.trim().to_owned();
    if let Some(source) = theme_source_from_name(&name) {
        return SyntaxThemeConfig {
            source,
            name: None,
            path: None,
        };
    }

    SyntaxThemeConfig {
        source: SyntaxThemeSource::Builtin,
        name: (!name.is_empty()).then_some(name),
        path: None,
    }
}

pub(crate) fn theme_config_from_table(table: StoredSyntaxThemeTable) -> SyntaxThemeConfig {
    let name = table
        .name
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty());
    let source = table
        .source
        .or_else(|| name.as_deref().and_then(theme_source_from_name))
        .or_else(|| table.path.as_ref().map(|_| SyntaxThemeSource::Base16))
        .unwrap_or_default();
    let name = if theme_source_from_name(name.as_deref().unwrap_or_default()).is_some() {
        None
    } else {
        name
    };

    SyntaxThemeConfig {
        source,
        name,
        path: table.path,
    }
}

pub(crate) fn theme_source_from_name(name: &str) -> Option<SyntaxThemeSource> {
    match name.trim().to_ascii_lowercase().as_str() {
        "ansi" | "terminal" => Some(SyntaxThemeSource::Ansi),
        "base16" => Some(SyntaxThemeSource::Base16),
        _ => None,
    }
}

pub(crate) fn limits_from_stored(stored: StoredSyntaxLimits) -> SyntaxLimits {
    let defaults = SyntaxLimits::default();
    SyntaxLimits {
        max_source_bytes: kib_or_default(stored.max_source_kib, defaults.max_source_bytes),
        max_line_bytes: kib_or_default(stored.max_line_kib, defaults.max_line_bytes),
        cache_entries: non_zero_or_default(stored.cache_entries, defaults.cache_entries),
        queue_entries: non_zero_or_default(stored.queue_entries, defaults.queue_entries),
        prefetch_viewports: stored
            .prefetch_viewports
            .unwrap_or(defaults.prefetch_viewports),
    }
}

pub(crate) fn kib_or_default(kib: Option<usize>, default: usize) -> usize {
    kib.and_then(|kib| kib.checked_mul(1024))
        .filter(|bytes| *bytes > 0)
        .unwrap_or(default)
}

pub(crate) fn non_zero_or_default(value: Option<usize>, default: usize) -> usize {
    value.filter(|value| *value > 0).unwrap_or(default)
}

pub(crate) fn enabled_language_set() -> MarkResult<BTreeSet<String>> {
    let settings = load_settings()?;
    let config = load_config()?;
    let installed = installed_language_set();
    let trusted = trusted_language_set(&installed, &config);
    Ok(enabled_language_set_for_mode(
        settings.mode,
        &config,
        &trusted,
    ))
}

pub(crate) fn enabled_language_set_for_mode(
    mode: SyntaxMode,
    config: &StoredSyntaxConfig,
    trusted: &BTreeSet<String>,
) -> BTreeSet<String> {
    match mode {
        SyntaxMode::Enabled => enabled_language_set_from_config(config),
        SyntaxMode::Builtin => bundled_highlight_language_set(),
        SyntaxMode::All => {
            let mut enabled = bundled_highlight_language_set();
            enabled.extend(trusted.iter().cloned());
            enabled
        }
    }
}

pub(crate) fn enabled_language_set_from_config(config: &StoredSyntaxConfig) -> BTreeSet<String> {
    let mut enabled = language_vec_to_set(&config.languages);
    enabled.extend(core_enabled_language_set());
    enabled
}

pub(crate) fn bundled_highlight_language_set() -> BTreeSet<String> {
    tree_sitter_language_pack::available_languages()
        .into_iter()
        .map(normalize_language_name)
        .filter(|language| {
            tree_sitter_language_pack::has_parser(language) && has_highlights(language)
        })
        .collect()
}

pub(crate) fn core_enabled_language_set() -> BTreeSet<String> {
    CORE_LANGUAGES
        .iter()
        .map(|language| normalize_language_name((*language).to_owned()))
        .filter(|language| tree_sitter_language_pack::has_parser(language))
        .collect()
}

pub(crate) fn reject_core_language_removal(requested: &BTreeSet<String>) -> MarkResult<()> {
    let core = core_enabled_language_set();
    let blocked = requested
        .intersection(&core)
        .cloned()
        .collect::<Vec<String>>();
    if blocked.is_empty() {
        return Ok(());
    }

    Err(MarkError::Usage(format!(
        "cannot remove core syntax languages: {}; use `mark --no-syntax` to disable syntax for a run",
        blocked.join(", ")
    )))
}

pub(crate) fn local_parser_language_set() -> BTreeSet<String> {
    let installed = installed_language_set();
    let mut languages = installed.clone();
    languages.extend(
        tree_sitter_language_pack::available_languages()
            .into_iter()
            .map(normalize_language_name)
            .filter(|language| {
                tree_sitter_language_pack::has_parser(language) || installed.contains(language)
            }),
    );
    languages
}

pub(crate) fn update_all_language_set(
    config: &StoredSyntaxConfig,
    installed: &BTreeSet<String>,
) -> BTreeSet<String> {
    let mut languages = language_vec_to_set(&config.languages);
    languages.extend(installed.iter().cloned());
    languages
}

pub(crate) fn installed_language_set() -> BTreeSet<String> {
    let mut installed = downloaded_language_set();
    if let Ok(config) = load_config() {
        installed.extend(
            config
                .parsers
                .iter()
                .filter(|artifact| {
                    artifact.source == CUSTOM_PARSER_SOURCE && artifact.path.exists()
                })
                .map(|artifact| normalize_language_name(artifact.language.clone())),
        );
    }
    installed
}

pub(crate) fn downloaded_language_set() -> BTreeSet<String> {
    tree_sitter_language_pack::downloaded_languages()
        .into_iter()
        .map(normalize_language_name)
        .collect()
}

pub(crate) fn trusted_language_set(
    installed: &BTreeSet<String>,
    config: &StoredSyntaxConfig,
) -> BTreeSet<String> {
    let artifacts = parser_artifact_map(config);
    installed
        .iter()
        .filter(|language| parser_artifact_is_trusted(language, &artifacts))
        .cloned()
        .collect()
}

pub(crate) fn parser_artifact_map(
    config: &StoredSyntaxConfig,
) -> BTreeMap<String, StoredParserArtifact> {
    config
        .parsers
        .iter()
        .cloned()
        .map(|mut artifact| {
            artifact.language = normalize_language_name(artifact.language);
            (artifact.language.clone(), artifact)
        })
        .collect()
}

pub(crate) fn language_vec_to_set(languages: &[String]) -> BTreeSet<String> {
    languages
        .iter()
        .cloned()
        .map(normalize_language_name)
        .filter(|language| !language.is_empty())
        .collect()
}

pub(crate) fn normalize_language_names(languages: &[String]) -> BTreeSet<String> {
    languages
        .iter()
        .cloned()
        .map(normalize_language_name)
        .filter(|language| !language.is_empty())
        .collect()
}

pub(crate) fn normalize_language_name(language: String) -> String {
    let language = language.trim().to_ascii_lowercase();
    if language.is_empty() {
        return String::new();
    }
    if let Some(language) = detect_language_from_basename(&language) {
        return language.to_owned();
    }
    if let Some(language) = tree_sitter_language_pack::detect_language_from_path(&language) {
        return language.to_owned();
    }
    let language = language.trim_start_matches('.');
    let language = language_alias(language).unwrap_or(language);
    tree_sitter_language_pack::detect_language_from_extension(language)
        .unwrap_or(language)
        .to_owned()
}

pub(crate) fn detect_language_name(path: &str) -> Option<&'static str> {
    detect_language_from_basename(path)
        .or_else(|| tree_sitter_language_pack::detect_language_from_path(path))
        .or_else(|| tree_sitter_language_pack::detect_language(path))
}

pub(crate) fn language_alias(language: &str) -> Option<&'static str> {
    LANGUAGE_ALIASES
        .iter()
        .find_map(|(alias, target)| (*alias == language).then_some(*target))
}

pub(crate) fn detect_language_from_basename(path: &str) -> Option<&'static str> {
    let name = Path::new(path).file_name()?.to_str()?;
    BASENAME_LANGUAGES
        .iter()
        .find_map(|(basename, language)| name.eq_ignore_ascii_case(basename).then_some(*language))
}

pub(crate) fn detect_custom_language_from_path(
    path: &str,
    extensions: &[StoredLanguageMapping],
    filenames: &[StoredLanguageMapping],
) -> Option<String> {
    let name = Path::new(path).file_name()?.to_str()?.to_ascii_lowercase();

    filenames
        .iter()
        .find(|mapping| name.eq_ignore_ascii_case(&mapping.pattern))
        .map(|mapping| mapping.language.clone())
        .or_else(|| {
            extensions
                .iter()
                .enumerate()
                .filter_map(|(index, mapping)| {
                    extension_mapping_match_len(&name, &mapping.pattern)
                        .map(|length| (length, index, mapping))
                })
                .max_by_key(|candidate| (candidate.0, candidate.1))
                .map(|(_, _, mapping)| mapping.language.clone())
        })
}

fn extension_mapping_match_len(filename: &str, extension: &str) -> Option<usize> {
    let extension = extension.trim_start_matches('.').to_ascii_lowercase();
    if extension.is_empty() {
        return None;
    }
    filename
        .ends_with(&format!(".{extension}"))
        .then_some(extension.len())
}

pub(crate) fn is_language_trusted(language: &str) -> bool {
    if tree_sitter_language_pack::has_parser(language) {
        return true;
    }

    let Ok(config) = load_config() else {
        return false;
    };
    let installed = installed_language_set();
    installed.contains(language)
        && parser_artifact_is_trusted(language, &parser_artifact_map(&config))
}

pub(crate) fn load_language_without_download(language: &str) -> Result<(), String> {
    let config = load_config().map_err(|error| error.to_string())?;
    load_language_with_config(language, &config).map(|_| ())
}

pub(crate) fn load_language_with_config(
    language: &str,
    config: &StoredSyntaxConfig,
) -> Result<tree_sitter_language_pack::Language, String> {
    let artifacts = parser_artifact_map(config);
    let artifact = (!tree_sitter_language_pack::has_parser(language)
        && parser_artifact_is_trusted(language, &artifacts))
    .then(|| artifacts.get(language))
    .flatten();
    load_language_with_artifact(language, artifact)
}

pub(crate) fn load_language_with_artifact(
    language: &str,
    artifact: Option<&StoredParserArtifact>,
) -> Result<tree_sitter_language_pack::Language, String> {
    let registry = artifact
        .and_then(|artifact| artifact.path.parent())
        .map(|parent| LanguageRegistry::with_libs_dir(parent.to_path_buf()))
        .unwrap_or_default();
    registry
        .get_language(language)
        .map_err(|error| error.to_string())
}

pub(crate) fn has_highlights(language: &str) -> bool {
    highlights_query(language).is_some()
}

pub(crate) fn highlights_query(language: &str) -> Option<Cow<'static, str>> {
    if let Some(query) = user_highlights_query(language) {
        return Some(Cow::Owned(query));
    }

    match language {
        "asm" => Some(Cow::Borrowed(ASM_HIGHLIGHTS_QUERY)),
        "commonlisp" => Some(Cow::Borrowed(COMMONLISP_HIGHLIGHTS_QUERY)),
        "ocaml" => Some(Cow::Borrowed(OCAML_HIGHLIGHTS_QUERY)),
        "typescript" | "tsx" => {
            tree_sitter_language_pack::get_highlights_query("javascript").map(Cow::Borrowed)
        }
        _ => tree_sitter_language_pack::get_highlights_query(language).map(Cow::Borrowed),
    }
}

pub(crate) fn user_highlights_query(language: &str) -> Option<String> {
    let path = user_highlights_query_path(language).ok()?;
    path.exists()
        .then(|| fs::read_to_string(path).ok())
        .flatten()
}

pub(crate) fn user_highlights_query_path(language: &str) -> MarkResult<PathBuf> {
    ensure_safe_language_name(language)?;
    Ok(queries_dir()?.join(language).join("highlights.scm"))
}

pub(crate) fn install_language(language: &str) -> MarkResult<Option<StoredParserArtifact>> {
    if tree_sitter_language_pack::has_parser(language) {
        tree_sitter_language_pack::get_language(language).map_err(|error| {
            MarkError::Usage(format!(
                "failed to load bundled tree-sitter language '{language}': {error}"
            ))
        })?;
        return Ok(None);
    }

    if !is_language_trusted(language)
        && let Some(path) = expected_cached_language_path(language)?
    {
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }

    // DownloadManager downloads and extracts without loading the native library.
    // Keep it that way until mark has seeded and re-verified its pinned manifest.
    write_trusted_parser_manifest()?;
    let cache = PathBuf::from(cache_dir()?);
    tree_sitter_language_pack::DownloadManager::with_cache_dir(&language_pack_version(), cache)
        .ensure_languages(&[language])
        .map_err(|error| {
            MarkError::Usage(format!(
                "failed to install tree-sitter language '{language}' from trusted parser lock: {error}"
            ))
        })?;
    verify_trusted_parser_manifest()?;

    let artifact = stored_parser_artifact(language)?;
    load_language_with_artifact(language, Some(&artifact)).map_err(|error| {
        MarkError::Usage(format!(
            "failed to load tree-sitter language '{language}' from verified parser cache: {error}"
        ))
    })?;

    Ok(Some(artifact))
}

#[derive(Debug)]
pub(crate) struct PreparedCustomParser {
    artifact: StoredParserArtifact,
    staged_path: PathBuf,
    staging_dir: Option<PathBuf>,
    destination: PathBuf,
}

impl PreparedCustomParser {
    pub(crate) fn artifact(&self) -> StoredParserArtifact {
        self.artifact.clone()
    }

    pub(crate) fn staged_artifact(&self) -> StoredParserArtifact {
        let mut artifact = self.artifact.clone();
        artifact.path = self.staged_path.clone();
        artifact
    }

    pub(crate) fn commit(mut self) -> MarkResult<InstalledFile> {
        let Some(_staging_dir) = self.staging_dir.as_ref() else {
            return Ok(InstalledFile::noop(self.destination.clone()));
        };
        let installed = replace_file_with_staged_path(&self.destination, &self.staged_path)?;
        self.staged_path = self.destination.clone();
        Ok(installed)
    }
}

impl Drop for PreparedCustomParser {
    fn drop(&mut self) {
        if let Some(staging_dir) = self.staging_dir.take() {
            let _ = fs::remove_dir_all(staging_dir);
        }
    }
}

#[derive(Debug)]
pub(crate) struct PreparedUserHighlightsQuery {
    pub(crate) contents: String,
    pub(crate) destination: PathBuf,
}

impl PreparedUserHighlightsQuery {
    pub(crate) fn commit(self) -> MarkResult<InstalledFile> {
        if let Some(parent) = self.destination.parent() {
            fs::create_dir_all(parent)?;
        }
        let staged_path = write_staged_file(&self.destination, self.contents.as_bytes())?;
        let staging_dir = staged_path.parent().map(Path::to_path_buf);
        let install_result = replace_file_with_staged_path(&self.destination, &staged_path);
        if let Some(staging_dir) = staging_dir {
            let _ = fs::remove_dir_all(staging_dir);
        }
        install_result
    }
}

#[derive(Debug)]
pub(crate) struct InstalledFile {
    destination: PathBuf,
    backup_dir: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    created: bool,
}

impl InstalledFile {
    fn noop(destination: PathBuf) -> Self {
        Self {
            destination,
            backup_dir: None,
            backup_path: None,
            created: false,
        }
    }

    pub(crate) fn rollback(mut self) -> MarkResult<()> {
        if let Some(backup_path) = self.backup_path.take() {
            match fs::remove_file(&self.destination) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            if let Err(error) = fs::rename(&backup_path, &self.destination) {
                self.backup_path = Some(backup_path);
                self.backup_dir = None;
                return Err(error.into());
            }
        } else if self.created {
            match fs::remove_file(&self.destination) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        self.created = false;
        if let Some(backup_dir) = self.backup_dir.take() {
            let _ = fs::remove_dir_all(backup_dir);
        }
        Ok(())
    }
}

impl Drop for InstalledFile {
    fn drop(&mut self) {
        if let Some(backup_dir) = self.backup_dir.take() {
            let _ = fs::remove_dir_all(backup_dir);
        }
    }
}

pub(crate) fn prepare_custom_parser(
    language: &str,
    parser_path: &Path,
) -> MarkResult<PreparedCustomParser> {
    ensure_safe_language_name(language)?;
    if tree_sitter_language_pack::has_parser(language) {
        return Err(MarkError::Usage(format!(
            "tree-sitter language '{language}' is bundled; custom parser overrides are not supported"
        )));
    }

    let source = parser_path;
    if !source.is_file() {
        return Err(MarkError::Usage(format!(
            "custom parser path does not exist or is not a file: {}",
            source.display()
        )));
    }

    let destination = custom_parser_path(language)?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }

    let source_canonical = source.canonicalize()?;
    let destination_canonical = destination.canonicalize().ok();
    let staged_path = if Some(source_canonical.as_path()) == destination_canonical.as_deref() {
        destination.clone()
    } else {
        staged_parser_path(source, &destination)?
    };
    let staging_dir = (staged_path != destination)
        .then(|| staged_path.parent().map(Path::to_path_buf))
        .flatten();
    let sha256 = match sha256_file(&staged_path) {
        Ok(sha256) => sha256,
        Err(error) => {
            if let Some(staging_dir) = staging_dir.as_ref() {
                let _ = fs::remove_dir_all(staging_dir);
            }
            return Err(error);
        }
    };

    let artifact = StoredParserArtifact {
        language: language.to_owned(),
        version: CUSTOM_PARSER_VERSION.to_owned(),
        sha256,
        installed_at_unix: unix_time_now(),
        source: CUSTOM_PARSER_SOURCE.to_owned(),
        path: destination.clone(),
    };
    let staged_artifact = StoredParserArtifact {
        path: staged_path.clone(),
        ..artifact.clone()
    };

    if let Err(error) = validate_custom_parser_candidate(language, &staged_artifact) {
        if let Some(staging_dir) = staging_dir.as_ref() {
            let _ = fs::remove_dir_all(staging_dir);
        }
        return Err(MarkError::Usage(format!(
            "failed to load custom tree-sitter language '{language}': {error}"
        )));
    }

    Ok(PreparedCustomParser {
        artifact,
        staged_path,
        staging_dir,
        destination,
    })
}

pub(crate) fn prepare_user_highlights_query(
    language: &str,
    query_path: &Path,
    config: &StoredSyntaxConfig,
    parser_artifact: Option<&StoredParserArtifact>,
) -> MarkResult<PreparedUserHighlightsQuery> {
    ensure_safe_language_name(language)?;
    if !query_path.is_file() {
        return Err(MarkError::Usage(format!(
            "highlights query path does not exist or is not a file: {}",
            query_path.display()
        )));
    }
    let query = fs::read_to_string(query_path)?;
    if let Some(parser_artifact) = parser_artifact {
        validate_custom_highlights_query_candidate(language, &query, parser_artifact)?;
    } else {
        validate_highlights_query_with_config(language, &query, config)?;
    }

    let destination = user_highlights_query_path(language)?;
    Ok(PreparedUserHighlightsQuery {
        contents: query,
        destination,
    })
}

pub(crate) fn validate_highlights_query(language: &str, query: &str) -> MarkResult<()> {
    let config = load_config()?;
    validate_highlights_query_with_config(language, query, &config)
}

pub(crate) fn validate_highlights_query_with_config(
    language: &str,
    query: &str,
    config: &StoredSyntaxConfig,
) -> MarkResult<()> {
    let language_fn = load_language_with_config(language, config)
        .map_err(|error| MarkError::Usage(format!("failed to load {language}: {error}")))?;
    validate_highlights_query_with_language(language, query, language_fn)
}

pub(crate) fn validate_highlights_query_with_artifact(
    language: &str,
    query: &str,
    artifact: Option<&StoredParserArtifact>,
) -> MarkResult<()> {
    let language_fn = load_language_with_artifact(language, artifact)
        .map_err(|error| MarkError::Usage(format!("failed to load {language}: {error}")))?;
    validate_highlights_query_with_language(language, query, language_fn)
}

fn validate_highlights_query_with_language(
    language: &str,
    query: &str,
    language_fn: tree_sitter_language_pack::Language,
) -> MarkResult<()> {
    let mut config =
        HighlightConfiguration::new(language_fn, language, query, "", "").map_err(|error| {
            MarkError::Usage(format!(
                "failed to configure {language} highlights: {error}"
            ))
        })?;
    config.configure(HIGHLIGHT_NAMES);
    Ok(())
}

fn validate_custom_parser_candidate(
    language: &str,
    artifact: &StoredParserArtifact,
) -> MarkResult<()> {
    validate_custom_parser_candidate_with_query(language, artifact, None)
}

fn validate_custom_highlights_query_candidate(
    language: &str,
    query: &str,
    artifact: &StoredParserArtifact,
) -> MarkResult<()> {
    validate_custom_parser_candidate_with_query(language, artifact, Some(query))
}

#[cfg(test)]
fn validate_custom_parser_candidate_with_query(
    language: &str,
    artifact: &StoredParserArtifact,
    query: Option<&str>,
) -> MarkResult<()> {
    if let Some(query) = query {
        validate_highlights_query_with_artifact(language, query, Some(artifact))
    } else {
        load_language_with_artifact(language, Some(artifact))
            .map(|_| ())
            .map_err(|error| MarkError::Usage(error.to_string()))
    }
}

#[cfg(not(test))]
fn validate_custom_parser_candidate_with_query(
    language: &str,
    artifact: &StoredParserArtifact,
    query: Option<&str>,
) -> MarkResult<()> {
    let mut command = Command::new(env::current_exe()?);
    command
        .env(VALIDATION_CHILD_ENV, "1")
        .env(VALIDATION_LANGUAGE_ENV, language)
        .env(VALIDATION_PARSER_ENV, &artifact.path)
        .stdin(if query.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if query.is_some() {
        command.env(VALIDATION_QUERY_ENV, "1");
    }

    let mut child = command.spawn()?;
    let write_result = if let Some(query) = query {
        child
            .stdin
            .take()
            .ok_or_else(|| {
                MarkError::Usage("failed to open syntax validation child stdin".to_owned())
            })
            .and_then(|mut stdin| stdin.write_all(query.as_bytes()).map_err(Into::into))
    } else {
        Ok(())
    };
    let output = child.wait_with_output()?;
    if output.status.success() {
        write_result?;
        if output.stdout.as_slice() == VALIDATION_CHILD_SUCCESS {
            return Ok(());
        }
        return Err(MarkError::Usage(
            "syntax validation child did not acknowledge validation request".to_owned(),
        ));
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let message = stderr.trim();
    if message.is_empty() {
        Err(MarkError::Usage(format!(
            "syntax validation child exited with {}",
            output.status
        )))
    } else {
        Err(MarkError::Usage(message.to_owned()))
    }
}

#[doc(hidden)]
pub fn run_validation_child_from_env() -> Option<MarkResult<()>> {
    env::var_os(VALIDATION_CHILD_ENV).map(|_| {
        run_validation_child()?;
        std::io::stdout()
            .lock()
            .write_all(VALIDATION_CHILD_SUCCESS)?;
        Ok(())
    })
}

fn run_validation_child() -> MarkResult<()> {
    let language = env::var(VALIDATION_LANGUAGE_ENV)
        .map_err(|_| MarkError::Usage("syntax validation child missing language".to_owned()))?;
    let parser_path = env::var_os(VALIDATION_PARSER_ENV)
        .map(PathBuf::from)
        .ok_or_else(|| {
            MarkError::Usage("syntax validation child missing parser path".to_owned())
        })?;
    let artifact = StoredParserArtifact {
        language: language.clone(),
        version: CUSTOM_PARSER_VERSION.to_owned(),
        sha256: String::new(),
        installed_at_unix: 0,
        source: CUSTOM_PARSER_SOURCE.to_owned(),
        path: parser_path,
    };

    if env::var_os(VALIDATION_QUERY_ENV).is_some() {
        let mut query = String::new();
        std::io::stdin().read_to_string(&mut query)?;
        validate_highlights_query_with_artifact(&language, &query, Some(&artifact))
    } else {
        load_language_with_artifact(&language, Some(&artifact))
            .map(|_| ())
            .map_err(|error| MarkError::Usage(error.to_string()))
    }
}

fn staged_parser_path(source: &Path, destination: &Path) -> MarkResult<PathBuf> {
    let staging_dir = create_sibling_temp_dir(destination, "stage")?;
    let filename = destination.file_name().ok_or_else(|| {
        MarkError::Usage(format!(
            "failed to resolve parser library filename for {}",
            destination.display()
        ))
    })?;
    let staged_path = staging_dir.join(filename);
    if let Err(error) = fs::copy(source, &staged_path) {
        let _ = fs::remove_dir_all(staging_dir);
        return Err(error.into());
    }
    Ok(staged_path)
}

fn write_staged_file(destination: &Path, contents: &[u8]) -> MarkResult<PathBuf> {
    let staging_dir = create_sibling_temp_dir(destination, "stage")?;
    let filename = destination.file_name().ok_or_else(|| {
        MarkError::Usage(format!(
            "failed to resolve filename for {}",
            destination.display()
        ))
    })?;
    let staged_path = staging_dir.join(filename);
    if let Err(error) = fs::write(&staged_path, contents) {
        let _ = fs::remove_dir_all(staging_dir);
        return Err(error.into());
    }
    Ok(staged_path)
}

fn replace_file_with_staged_path(
    destination: &Path,
    staged_path: &Path,
) -> MarkResult<InstalledFile> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut backup_dir = None;
    let mut backup_path = None;
    if destination.exists() {
        let dir = create_sibling_temp_dir(destination, "backup")?;
        let filename = destination.file_name().ok_or_else(|| {
            MarkError::Usage(format!(
                "failed to resolve filename for {}",
                destination.display()
            ))
        })?;
        let path = dir.join(filename);
        if let Err(error) = fs::rename(destination, &path) {
            let _ = fs::remove_dir_all(dir);
            return Err(error.into());
        }
        backup_dir = Some(dir);
        backup_path = Some(path);
    }

    if let Err(error) = fs::rename(staged_path, destination) {
        let restored_backup = backup_path
            .as_ref()
            .map(|path| fs::rename(path, destination).is_ok())
            .unwrap_or(true);
        if restored_backup && let Some(dir) = backup_dir.as_ref() {
            let _ = fs::remove_dir_all(dir);
        }
        return Err(error.into());
    }

    Ok(InstalledFile {
        destination: destination.to_path_buf(),
        backup_dir,
        created: backup_path.is_none(),
        backup_path,
    })
}

fn create_sibling_temp_dir(destination: &Path, label: &str) -> MarkResult<PathBuf> {
    let parent = destination.parent().ok_or_else(|| {
        MarkError::Usage(format!(
            "failed to resolve parent directory for {}",
            destination.display()
        ))
    })?;
    let filename = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            MarkError::Usage(format!(
                "failed to resolve filename for {}",
                destination.display()
            ))
        })?;
    for attempt in 0..16 {
        let path = parent.join(format!(
            ".{filename}.{label}.{}.{}.{attempt}",
            std::process::id(),
            unix_time_now()
        ));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(MarkError::Usage(format!(
        "failed to create temporary directory next to {}",
        destination.display()
    )))
}

pub(crate) fn custom_parser_path(language: &str) -> MarkResult<PathBuf> {
    ensure_safe_language_name(language)?;
    let filename = expected_cached_language_path(language)?
        .and_then(|path| path.file_name().map(PathBuf::from))
        .ok_or_else(|| {
            MarkError::Usage(format!(
                "failed to resolve parser library filename for tree-sitter language '{language}'"
            ))
        })?;
    Ok(parsers_dir()?.join(filename))
}

pub(crate) fn ensure_safe_language_name(language: &str) -> MarkResult<()> {
    if !language.is_empty()
        && language
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
    {
        return Ok(());
    }

    Err(MarkError::Usage(format!(
        "language names must use lowercase letters, digits, or underscores: {language}"
    )))
}

pub(crate) fn normalize_custom_extension(extension: &str) -> MarkResult<String> {
    let extension = extension
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase();
    if !extension.is_empty()
        && !extension.contains('/')
        && !extension.contains('\\')
        && !extension.split('.').any(str::is_empty)
    {
        return Ok(extension);
    }

    Err(MarkError::Usage(format!(
        "extension mappings must be extension tokens without path separators: {extension}"
    )))
}

pub(crate) fn normalize_custom_filename(filename: &str) -> MarkResult<String> {
    let filename = filename.trim();
    if !filename.is_empty()
        && !filename.contains('/')
        && !filename.contains('\\')
        && Path::new(filename)
            .file_name()
            .and_then(|name| name.to_str())
            == Some(filename)
    {
        return Ok(filename.to_owned());
    }

    Err(MarkError::Usage(format!(
        "filename mappings must be bare filenames without path separators: {filename}"
    )))
}

pub(crate) fn upsert_extension_mappings(
    mappings: &mut Vec<StoredLanguageMapping>,
    language: &str,
    extensions: &[String],
) -> MarkResult<Vec<String>> {
    let mut added = Vec::new();
    for extension in extensions {
        let pattern = normalize_custom_extension(extension)?;
        upsert_mapping(mappings, language, &pattern);
        added.push(pattern);
    }
    Ok(added)
}

pub(crate) fn upsert_filename_mappings(
    mappings: &mut Vec<StoredLanguageMapping>,
    language: &str,
    filenames: &[String],
) -> MarkResult<Vec<String>> {
    let mut added = Vec::new();
    for filename in filenames {
        let pattern = normalize_custom_filename(filename)?;
        upsert_mapping(mappings, language, &pattern);
        added.push(pattern);
    }
    Ok(added)
}

pub(crate) fn upsert_mapping(
    mappings: &mut Vec<StoredLanguageMapping>,
    language: &str,
    pattern: &str,
) {
    mappings.retain(|mapping| !mapping.pattern.eq_ignore_ascii_case(pattern));
    mappings.push(StoredLanguageMapping {
        pattern: pattern.to_owned(),
        language: language.to_owned(),
    });
}

pub(crate) fn stored_parser_artifact(language: &str) -> MarkResult<StoredParserArtifact> {
    let path = expected_cached_language_path(language)?.ok_or_else(|| {
        MarkError::Usage(format!(
            "failed to resolve parser artifact path for tree-sitter language '{language}'"
        ))
    })?;
    if !path.exists() {
        return Err(MarkError::Usage(format!(
            "tree-sitter language '{language}' loaded, but parser artifact is missing at {}",
            path.display()
        )));
    }

    Ok(StoredParserArtifact {
        language: language.to_owned(),
        version: language_pack_version(),
        sha256: sha256_file(&path)?,
        installed_at_unix: unix_time_now(),
        source: ARTIFACT_SOURCE.to_owned(),
        path,
    })
}

pub(crate) fn upsert_parser_artifact(
    config: &mut StoredSyntaxConfig,
    language: &str,
    artifact: Option<StoredParserArtifact>,
) {
    config
        .parsers
        .retain(|existing| existing.language != language);
    if let Some(artifact) = artifact {
        config.parsers.push(artifact);
    }
}

pub(crate) fn parser_artifact_is_trusted(
    language: &str,
    artifacts: &BTreeMap<String, StoredParserArtifact>,
) -> bool {
    let Some(artifact) = artifacts.get(language) else {
        return false;
    };
    if artifact.source == CUSTOM_PARSER_SOURCE {
        let Ok(expected_path) = custom_parser_path(language) else {
            return false;
        };
        return artifact.version == CUSTOM_PARSER_VERSION
            && artifact.path == expected_path
            && artifact.path.exists()
            && sha256_file(&artifact.path).is_ok_and(|sha256| sha256 == artifact.sha256);
    }

    if artifact.version != language_pack_version() || artifact.source != ARTIFACT_SOURCE {
        return false;
    }
    let Ok(Some(expected_path)) = expected_cached_language_path(language) else {
        return false;
    };
    if artifact.path != expected_path || !artifact.path.exists() {
        return false;
    }
    sha256_file(&artifact.path).is_ok_and(|sha256| sha256 == artifact.sha256)
}

pub(crate) fn expected_cached_language_path(language: &str) -> MarkResult<Option<PathBuf>> {
    let cache = PathBuf::from(cache_dir()?);
    Ok(Some(
        tree_sitter_language_pack::DownloadManager::with_cache_dir(&language_pack_version(), cache)
            .lib_path(language),
    ))
}

pub(crate) fn write_trusted_parser_manifest() -> MarkResult<()> {
    let path = trusted_parser_manifest_path()?;
    if path.exists()
        && sha256_file(&path).is_ok_and(|sha256| sha256 == TRUSTED_PARSER_MANIFEST_SHA256)
    {
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, TRUSTED_PARSER_MANIFEST.as_bytes())?;
    Ok(())
}

pub(crate) fn verify_trusted_parser_manifest() -> MarkResult<()> {
    let path = trusted_parser_manifest_path()?;
    let sha256 = sha256_file(&path)?;
    if sha256 == TRUSTED_PARSER_MANIFEST_SHA256 {
        return Ok(());
    }

    Err(MarkError::Usage(format!(
        "tree-sitter parser manifest at {} did not match shipped parser lock (expected {}, got {})",
        path.display(),
        TRUSTED_PARSER_MANIFEST_SHA256,
        sha256
    )))
}

pub(crate) fn trusted_parser_manifest_path() -> MarkResult<PathBuf> {
    let cache = PathBuf::from(cache_dir()?);
    cache
        .parent()
        .map(|path| path.join("manifest.json"))
        .ok_or_else(|| MarkError::Usage("tree-sitter cache directory has no parent".to_owned()))
}

pub(crate) fn sha256_file(path: &Path) -> MarkResult<String> {
    let bytes = fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(hex_encode(&hasher.finalize()))
}

pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

pub(crate) fn unix_time_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

pub(crate) fn language_pack_version() -> String {
    cache_dir()
        .ok()
        .and_then(|cache| {
            Path::new(&cache)
                .parent()
                .and_then(|parent| parent.file_name())
                .and_then(|version| version.to_str())
                .and_then(|version| version.strip_prefix('v'))
                .map(str::to_owned)
        })
        .unwrap_or_else(|| LANGUAGE_PACK_VERSION.to_owned())
}

pub(crate) fn remove_cached_language(language: &str) -> MarkResult<bool> {
    let cache = PathBuf::from(cache_dir()?);
    let mut candidates = BTreeSet::new();
    if let Some(path) = cached_language_path(&cache, language) {
        candidates.insert(path);
    }
    if let Ok(path) = custom_parser_path(language) {
        candidates.insert(path);
    }
    candidates.extend(scan_cached_language_paths(&cache, language));

    let mut removed = false;
    for path in candidates {
        match fs::remove_file(&path) {
            Ok(()) => removed = true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(removed)
}

pub(crate) fn cached_language_path(cache: &Path, language: &str) -> Option<PathBuf> {
    let version = cache
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|version| version.to_str())
        .and_then(|version| version.strip_prefix('v'))?;
    Some(
        tree_sitter_language_pack::DownloadManager::with_cache_dir(version, cache.to_path_buf())
            .lib_path(language),
    )
}

pub(crate) fn scan_cached_language_paths(cache: &Path, language: &str) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(cache) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| cached_filename_matches_language(name, language))
        })
        .collect()
}

pub(crate) fn cached_filename_matches_language(name: &str, language: &str) -> bool {
    let name = name.strip_prefix("lib").unwrap_or(name);
    let Some(name) = name
        .strip_prefix("tree_sitter_")
        .or_else(|| name.strip_prefix("tree-sitter-"))
    else {
        return false;
    };
    let Some(name) = name
        .strip_suffix(".so")
        .or_else(|| name.strip_suffix(".dylib"))
        .or_else(|| name.strip_suffix(".dll"))
    else {
        return false;
    };

    name == language || name.replace('_', "") == language.replace('_', "")
}

#[cfg(test)]
mod storage_tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_DIR: AtomicU64 = AtomicU64::new(0);

    fn temp_test_dir(name: &str) -> PathBuf {
        let path = env::temp_dir().join(format!(
            "mark-syntax-{name}-{}-{}",
            std::process::id(),
            NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("test dir should be created");
        path
    }

    #[test]
    fn staged_parser_copy_uses_destination_filename_without_replacing_destination() {
        let dir = temp_test_dir("parser-stage");
        let source = dir.join("candidate-parser");
        let destination = dir.join("libtree_sitter_customlang.dylib");
        fs::write(&source, b"candidate").expect("source parser should be written");
        fs::write(&destination, b"trusted").expect("destination parser should be written");

        let staged_path =
            staged_parser_path(&source, &destination).expect("parser should be staged");

        assert_eq!(staged_path.file_name(), destination.file_name());
        assert_eq!(fs::read(&destination).unwrap(), b"trusted");
        assert_eq!(fs::read(&staged_path).unwrap(), b"candidate");

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn staged_file_replacement_rolls_back_existing_destination() {
        let dir = temp_test_dir("rollback-existing");
        let destination = dir.join("highlights.scm");
        fs::write(&destination, b"trusted").expect("destination should be written");

        let staged_path =
            write_staged_file(&destination, b"candidate").expect("file should be staged");
        assert_eq!(fs::read(&destination).unwrap(), b"trusted");

        let installed = replace_file_with_staged_path(&destination, &staged_path)
            .expect("staged file should be promoted");
        assert_eq!(fs::read(&destination).unwrap(), b"candidate");

        installed
            .rollback()
            .expect("rollback should restore backup");
        assert_eq!(fs::read(&destination).unwrap(), b"trusted");

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn staged_file_replacement_rolls_back_created_destination() {
        let dir = temp_test_dir("rollback-created");
        let destination = dir.join("highlights.scm");

        let staged_path =
            write_staged_file(&destination, b"candidate").expect("file should be staged");
        let installed = replace_file_with_staged_path(&destination, &staged_path)
            .expect("staged file should be promoted");
        assert_eq!(fs::read(&destination).unwrap(), b"candidate");

        installed
            .rollback()
            .expect("rollback should remove created destination");
        assert!(!destination.exists());

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn prepared_highlights_query_does_not_replace_until_commit() {
        let dir = temp_test_dir("query-commit");
        let destination = dir.join("highlights.scm");
        fs::write(&destination, b"trusted").expect("destination should be written");
        let query = PreparedUserHighlightsQuery {
            contents: "candidate".to_owned(),
            destination: destination.clone(),
        };

        assert_eq!(fs::read_to_string(&destination).unwrap(), "trusted");

        query.commit().expect("query should be installed");
        assert_eq!(fs::read_to_string(&destination).unwrap(), "candidate");

        let _ = fs::remove_dir_all(dir);
    }
}
