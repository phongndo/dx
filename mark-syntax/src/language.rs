use std::collections::BTreeSet;

use crate::{
    CUSTOM_PARSER_SOURCE, InstalledFile, PreparedCustomParser, PreparedUserHighlightsQuery,
    StoredSyntaxConfig, SyntaxAddOptions, SyntaxAddResult, SyntaxAvailableFilter,
    SyntaxCleanResult, SyntaxDoctorIssue, SyntaxDoctorReport, SyntaxLanguageStatus,
    SyntaxParserArtifact, SyntaxRemoveResult, SyntaxUpdateResult, core_enabled_language_set,
    downloaded_language_set, enabled_language_set, enabled_language_set_for_mode, has_highlights,
    highlights_query, install_language, installed_language_set, language_pack_version,
    language_vec_to_set, load_config, load_language_without_download, load_settings,
    local_parser_language_set, normalize_language_names, parser_artifact_map,
    prepare_custom_parser, prepare_user_highlights_query, reject_core_language_removal,
    remove_cached_language, save_config, trusted_language_set, update_all_language_set,
    upsert_extension_mappings, upsert_filename_mappings, upsert_parser_artifact,
    user_highlights_query_path, validate_highlights_query,
};
use mark_core::{MarkError, MarkResult};

pub fn available_languages(filter: SyntaxAvailableFilter) -> MarkResult<Vec<String>> {
    match filter {
        SyntaxAvailableFilter::All => {
            let mut languages =
                tree_sitter_language_pack::manifest_languages().map_err(|error| {
                    MarkError::Usage(format!("failed to list tree-sitter languages: {error}"))
                })?;
            if let Ok(config) = load_config() {
                languages.extend(
                    config
                        .parsers
                        .iter()
                        .map(|artifact| artifact.language.clone()),
                );
                languages.extend(
                    config
                        .extensions
                        .iter()
                        .map(|mapping| mapping.language.clone()),
                );
                languages.extend(
                    config
                        .filenames
                        .iter()
                        .map(|mapping| mapping.language.clone()),
                );
            }
            languages.sort();
            languages.dedup();
            Ok(languages)
        }
        SyntaxAvailableFilter::Installed => Ok(local_parser_language_set().into_iter().collect()),
        SyntaxAvailableFilter::Enabled => enabled_languages(),
    }
}

pub fn enabled_languages() -> MarkResult<Vec<String>> {
    Ok(enabled_language_set()?.into_iter().collect())
}

pub fn installed_languages() -> Vec<String> {
    installed_language_set().into_iter().collect()
}

pub fn language_statuses() -> MarkResult<Vec<SyntaxLanguageStatus>> {
    let settings = load_settings()?;
    let config = load_config()?;
    let installed = installed_language_set();
    let trusted = trusted_language_set(&installed, &config);
    let enabled = enabled_language_set_for_mode(settings.mode, &config, &trusted);
    let artifacts = parser_artifact_map(&config);
    let pack_version = language_pack_version();
    let mut languages = enabled
        .union(&installed)
        .cloned()
        .collect::<BTreeSet<String>>();
    languages.extend(core_enabled_language_set());

    if languages.is_empty() {
        languages.extend(installed.iter().cloned());
    }

    Ok(languages
        .into_iter()
        .map(|language| {
            let built_in = tree_sitter_language_pack::has_parser(&language);
            let artifact = (!built_in)
                .then(|| artifacts.get(&language).map(SyntaxParserArtifact::from))
                .flatten();
            let artifact_source = artifact.as_ref().map(|artifact| artifact.source.clone());
            let artifact_version = artifact.as_ref().map(|artifact| artifact.version.clone());
            SyntaxLanguageStatus {
                enabled: enabled.contains(&language),
                installed: built_in || installed.contains(&language),
                trusted: built_in || trusted.contains(&language),
                has_highlights: has_highlights(&language),
                version: if built_in {
                    Some(pack_version.clone())
                } else {
                    artifact_version
                },
                source: if built_in {
                    Some("bundled".to_owned())
                } else {
                    artifact_source
                },
                artifact,
                language,
            }
        })
        .collect())
}

pub fn add_languages(languages: &[String]) -> MarkResult<SyntaxAddResult> {
    add_languages_with_options(languages, SyntaxAddOptions::default())
}

pub fn add_languages_with_options(
    languages: &[String],
    options: SyntaxAddOptions,
) -> MarkResult<SyntaxAddResult> {
    if languages.is_empty() {
        return Err(MarkError::Usage("provide at least one language".to_owned()));
    }

    let has_custom_options = options.parser.is_some()
        || options.query.is_some()
        || !options.extensions.is_empty()
        || !options.filenames.is_empty();
    if has_custom_options && languages.len() != 1 {
        return Err(MarkError::Usage(
            "use --parser, --query, --ext, or --filename with exactly one language".to_owned(),
        ));
    }

    let requested = normalize_language_names(languages);
    let mut config = load_config()?;
    let mut enabled = language_vec_to_set(&config.languages);
    let mut added = Vec::new();
    let mut already_enabled = Vec::new();
    let mut without_highlights = Vec::new();
    let mut custom_parsers = Vec::new();
    let mut custom_queries = Vec::new();
    let mut custom_mappings = Vec::new();
    let mut pending_custom_parser = None;
    let mut pending_query = None;

    for language in requested {
        if let Some(parser_path) = options.parser.as_deref() {
            let parser = prepare_custom_parser(&language, parser_path)?;
            let artifact = parser.artifact();
            custom_parsers.push(language.clone());
            upsert_parser_artifact(&mut config, &language, Some(artifact));
            pending_custom_parser = Some(parser);
        } else if !has_custom_parser_artifact(&config, &language) {
            let artifact = install_language(&language)?;
            upsert_parser_artifact(&mut config, &language, artifact);
        }

        if let Some(query_path) = options.query.as_deref() {
            let staged_parser_artifact = pending_custom_parser
                .as_ref()
                .map(|parser| parser.staged_artifact());
            let query = prepare_user_highlights_query(
                &language,
                query_path,
                &config,
                staged_parser_artifact.as_ref(),
            )?;
            custom_queries.push(language.clone());
            pending_query = Some(query);
        }

        let extension_mappings =
            upsert_extension_mappings(&mut config.extensions, &language, &options.extensions)?;
        custom_mappings.extend(
            extension_mappings
                .into_iter()
                .map(|extension| format!("*.{extension} -> {language}")),
        );
        let filename_mappings =
            upsert_filename_mappings(&mut config.filenames, &language, &options.filenames)?;
        custom_mappings.extend(
            filename_mappings
                .into_iter()
                .map(|filename| format!("{filename} -> {language}")),
        );

        if options.query.is_none() && !has_highlights(&language) {
            without_highlights.push(language.clone());
        }

        if enabled.insert(language.clone()) {
            added.push(language);
        } else {
            already_enabled.push(language);
        }
    }

    config.languages = enabled.into_iter().collect();
    commit_prepared_syntax_add(&config, pending_custom_parser, pending_query, save_config)?;

    Ok(SyntaxAddResult {
        added,
        already_enabled,
        without_highlights,
        custom_parsers,
        custom_queries,
        custom_mappings,
    })
}

pub(crate) fn commit_prepared_syntax_add<F>(
    config: &StoredSyntaxConfig,
    pending_custom_parser: Option<PreparedCustomParser>,
    pending_query: Option<PreparedUserHighlightsQuery>,
    save: F,
) -> MarkResult<()>
where
    F: FnOnce(&StoredSyntaxConfig) -> MarkResult<()>,
{
    let installed_custom_parser = pending_custom_parser
        .map(PreparedCustomParser::commit)
        .transpose()?;

    let installed_query = match pending_query {
        Some(query) => match query.commit() {
            Ok(installed) => Some(installed),
            Err(error) => {
                rollback_installed_files(None, installed_custom_parser);
                return Err(error);
            }
        },
        None => None,
    };

    if let Err(error) = save(config) {
        rollback_installed_files(installed_query, installed_custom_parser);
        return Err(error);
    }

    Ok(())
}

fn rollback_installed_files(
    installed_query: Option<InstalledFile>,
    installed_custom_parser: Option<InstalledFile>,
) {
    if let Some(installed_query) = installed_query {
        let _ = installed_query.rollback();
    }
    if let Some(installed_custom_parser) = installed_custom_parser {
        let _ = installed_custom_parser.rollback();
    }
}

pub(crate) fn has_custom_parser_artifact(config: &StoredSyntaxConfig, language: &str) -> bool {
    parser_artifact_map(config)
        .get(language)
        .is_some_and(|artifact| artifact.source == CUSTOM_PARSER_SOURCE)
}

pub fn update_languages(languages: &[String], all: bool) -> MarkResult<SyntaxUpdateResult> {
    if all && !languages.is_empty() {
        return Err(MarkError::Usage(
            "use `mark syntax update --all` without language names".to_owned(),
        ));
    }
    if !all && languages.is_empty() {
        return Err(MarkError::Usage(
            "provide at least one language or use --all".to_owned(),
        ));
    }

    let mut config = load_config()?;
    let configured = language_vec_to_set(&config.languages);
    let installed = installed_language_set();
    let artifacts = parser_artifact_map(&config);
    let requested = if all {
        update_all_language_set(&config, &installed)
    } else {
        normalize_language_names(languages)
    };
    let mut result = SyntaxUpdateResult::default();

    for language in requested {
        let custom_parser = artifacts
            .get(&language)
            .is_some_and(|artifact| artifact.source == CUSTOM_PARSER_SOURCE);

        if !custom_parser && !tree_sitter_language_pack::has_language(&language) {
            if all {
                result.unavailable.push(language);
                continue;
            }
            return Err(MarkError::Usage(format!(
                "tree-sitter language '{language}' is not known"
            )));
        }

        if record_update_parser_result(&mut result, &language, custom_parser) {
            continue;
        }

        if tree_sitter_language_pack::has_parser(&language) {
            result.bundled.push(language);
            continue;
        }

        if !installed.contains(&language) && !configured.contains(&language) {
            result.not_installed.push(language);
            continue;
        }

        remove_cached_language(&language)?;
        let artifact = install_language(&language)?;
        upsert_parser_artifact(&mut config, &language, artifact);
        result.updated.push(language);
    }

    if !result.updated.is_empty() {
        save_config(&config)?;
    }

    Ok(result)
}

pub(crate) fn record_update_parser_result(
    result: &mut SyntaxUpdateResult,
    language: &str,
    custom_parser: bool,
) -> bool {
    if !has_highlights(language) {
        result.without_highlights.push(language.to_owned());
    }

    if custom_parser {
        result.custom.push(language.to_owned());
        return true;
    }

    false
}

pub fn remove_languages(languages: &[String]) -> MarkResult<SyntaxRemoveResult> {
    if languages.is_empty() {
        return Err(MarkError::Usage("provide at least one language".to_owned()));
    }

    let requested = normalize_language_names(languages);
    reject_core_language_removal(&requested)?;
    let mut config = load_config()?;
    let mut enabled = language_vec_to_set(&config.languages);
    let mut removed = Vec::new();
    let mut missing = Vec::new();
    let mut cache_deleted = Vec::new();
    let mut cache_missing = Vec::new();

    for language in &requested {
        if enabled.remove(language.as_str()) {
            removed.push(language.clone());
        } else {
            missing.push(language.clone());
        }
    }
    config
        .parsers
        .retain(|artifact| !requested.contains(&artifact.language));
    config
        .extensions
        .retain(|mapping| !requested.contains(&mapping.language));
    config
        .filenames
        .retain(|mapping| !requested.contains(&mapping.language));

    config.languages = enabled.into_iter().collect();
    save_config(&config)?;

    for language in requested {
        if remove_cached_language(&language)? {
            cache_deleted.push(language);
        } else {
            cache_missing.push(language);
        }
    }

    Ok(SyntaxRemoveResult {
        removed,
        missing,
        cache_deleted,
        cache_missing,
    })
}

pub fn clean_cache() -> MarkResult<SyntaxCleanResult> {
    let parser_artifacts_removed = downloaded_language_set().len();
    let mut config = load_config()?;
    let artifact_records_before = config.parsers.len();
    let enabled_languages_kept = language_vec_to_set(&config.languages).len();

    tree_sitter_language_pack::clean_cache()
        .map_err(|error| MarkError::Usage(format!("failed to clean tree-sitter cache: {error}")))?;
    config
        .parsers
        .retain(|artifact| artifact.source == CUSTOM_PARSER_SOURCE);
    let artifact_records_removed = artifact_records_before.saturating_sub(config.parsers.len());
    save_config(&config)?;

    Ok(SyntaxCleanResult {
        parser_artifacts_removed,
        artifact_records_removed,
        enabled_languages_kept,
    })
}

pub fn doctor() -> MarkResult<SyntaxDoctorReport> {
    let statuses = language_statuses()?;
    let issues = doctor_issues(&statuses);

    Ok(SyntaxDoctorReport { statuses, issues })
}

pub(crate) fn doctor_issues(statuses: &[SyntaxLanguageStatus]) -> Vec<SyntaxDoctorIssue> {
    let mut issues = Vec::new();

    for status in statuses {
        if !status.enabled {
            continue;
        }
        let custom_parser = status.source.as_deref() == Some(CUSTOM_PARSER_SOURCE);
        if !custom_parser && !tree_sitter_language_pack::has_language(&status.language) {
            issues.push(SyntaxDoctorIssue {
                language: status.language.clone(),
                message: "enabled in config, but language is not known; run `mark syntax rm`"
                    .to_owned(),
            });
            continue;
        }
        if !status.installed {
            issues.push(SyntaxDoctorIssue {
                language: status.language.clone(),
                message:
                    "enabled in config, but parser cache file is missing; run `mark syntax add`"
                        .to_owned(),
            });
            continue;
        }
        if !status.trusted {
            issues.push(SyntaxDoctorIssue {
                language: status.language.clone(),
                message:
                    "parser exists, but no matching trusted checksum is recorded; run `mark syntax add`"
                        .to_owned(),
            });
            continue;
        }
        if !status.has_highlights {
            let query_path = user_highlights_query_path(&status.language)
                .ok()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "~/.config/mark/queries/<language>/highlights.scm".to_owned());
            issues.push(SyntaxDoctorIssue {
                language: status.language.clone(),
                message: format!(
                    "parser is available, but no highlights query exists; add one at {query_path}"
                ),
            });
        } else if let Some(query) = highlights_query(&status.language)
            && let Err(error) = validate_highlights_query(&status.language, query.as_ref())
        {
            issues.push(SyntaxDoctorIssue {
                language: status.language.clone(),
                message: format!("highlights query failed to load: {error}"),
            });
        }
        if let Err(error) = load_language_without_download(&status.language) {
            issues.push(SyntaxDoctorIssue {
                language: status.language.clone(),
                message: format!("parser exists, but failed to load without downloading: {error}"),
            });
        }
    }

    issues
}
