use anyhow::Result;
use turbo_tasks::{TryFlatJoinIterExt, Vc};
use turbo_tasks_fs::{glob::Glob, FileJsonContent, FileSystemPath};
use turbopack_core::{
    asset::Asset,
    chunk::ChunkableModule,
    error::PrettyPrintError,
    issue::{Issue, IssueExt, IssueSeverity, IssueStage, OptionStyledString, StyledString},
    module::Module,
    resolve::{find_context_file, package_json, FindContextFileResult},
};

use crate::references::{
    async_module::OptionAsyncModule,
    esm::{EsmExport, EsmExports},
};

#[turbo_tasks::value_trait]
pub trait EcmascriptChunkPlaceable: ChunkableModule + Module + Asset {
    fn get_exports(self: Vc<Self>) -> Vc<EcmascriptExports>;
    fn get_async_module(self: Vc<Self>) -> Vc<OptionAsyncModule> {
        Vc::cell(None)
    }
    fn is_marked_as_side_effect_free(
        self: Vc<Self>,
        side_effect_free_packages: Vc<Vec<String>>,
    ) -> Vc<bool> {
        is_marked_as_side_effect_free(self.ident().path(), side_effect_free_packages)
    }
}

#[turbo_tasks::value]
enum SideEffectsValue {
    None,
    Constant(bool),
    Glob(Vc<Glob>),
}

#[turbo_tasks::function]
async fn side_effects_from_package_json(
    package_json: Vc<FileSystemPath>,
    side_effect_free_packages: Vc<Vec<String>>,
) -> Result<Vc<SideEffectsValue>> {
    if let FileJsonContent::Content(content) = &*package_json.read_json().await? {
        if let Some(pacakge_name) = content.get("name").and_then(|v| v.as_str()) {
            if side_effect_free_packages
                .await?
                .iter()
                .any(|v| pacakge_name == v)
            {
                return Ok(SideEffectsValue::Constant(false).cell());
            }
        }

        if let Some(side_effects) = content.get("sideEffects") {
            if let Some(side_effects) = side_effects.as_bool() {
                return Ok(SideEffectsValue::Constant(side_effects).cell());
            } else if let Some(side_effects) = side_effects.as_array() {
                let globs = side_effects
                    .iter()
                    .filter_map(|side_effect| {
                        if let Some(side_effect) = side_effect.as_str() {
                            if side_effect.contains('/') {
                                Some(Glob::new(side_effect.to_string()))
                            } else {
                                Some(Glob::new(format!("**/{side_effect}")))
                            }
                        } else {
                            SideEffectsInPackageJsonIssue {
                                path: package_json,
                                description: Some(
                                    StyledString::Text(format!(
                                        "Each element in sideEffects must be a string, but found \
                                         {:?}",
                                        side_effect
                                    ))
                                    .cell(),
                                ),
                            }
                            .cell()
                            .emit();
                            None
                        }
                    })
                    .map(|glob| async move {
                        match glob.resolve().await {
                            Ok(glob) => Ok(Some(glob)),
                            Err(err) => {
                                SideEffectsInPackageJsonIssue {
                                    path: package_json,
                                    description: Some(
                                        StyledString::Text(format!(
                                            "Invalid glob in sideEffects: {}",
                                            PrettyPrintError(&err)
                                        ))
                                        .cell(),
                                    ),
                                }
                                .cell()
                                .emit();
                                Ok(None)
                            }
                        }
                    })
                    .try_flat_join()
                    .await?;
                return Ok(
                    SideEffectsValue::Glob(Glob::alternatives(globs).resolve().await?).cell(),
                );
            } else {
                SideEffectsInPackageJsonIssue {
                    path: package_json,
                    description: Some(
                        StyledString::Text(format!(
                            "sideEffects must be a boolean or an array, but found {:?}",
                            side_effects
                        ))
                        .cell(),
                    ),
                }
                .cell()
                .emit();
            }
        }
    }
    Ok(SideEffectsValue::None.cell())
}

#[turbo_tasks::value]
struct SideEffectsInPackageJsonIssue {
    path: Vc<FileSystemPath>,
    description: Option<Vc<StyledString>>,
}

#[turbo_tasks::value_impl]
impl Issue for SideEffectsInPackageJsonIssue {
    #[turbo_tasks::function]
    fn stage(&self) -> Vc<IssueStage> {
        IssueStage::Parse.into()
    }

    #[turbo_tasks::function]
    fn severity(&self) -> Vc<IssueSeverity> {
        IssueSeverity::Warning.cell()
    }

    #[turbo_tasks::function]
    fn file_path(&self) -> Vc<FileSystemPath> {
        self.path
    }

    #[turbo_tasks::function]
    fn title(&self) -> Vc<StyledString> {
        StyledString::Text("Invalid value for sideEffects in package.json".to_string()).cell()
    }

    #[turbo_tasks::function]
    fn description(&self) -> Vc<OptionStyledString> {
        Vc::cell(self.description)
    }
}

#[turbo_tasks::function]
pub async fn is_marked_as_side_effect_free(
    path: Vc<FileSystemPath>,
    side_effect_free_packages: Vc<Vec<String>>,
) -> Result<Vc<bool>> {
    let find_package_json: turbo_tasks::ReadRef<FindContextFileResult> =
        find_context_file(path.parent(), package_json()).await?;

    if let FindContextFileResult::Found(package_json, _) = *find_package_json {
        match *side_effects_from_package_json(package_json, side_effect_free_packages).await? {
            SideEffectsValue::None => {}
            SideEffectsValue::Constant(side_effects) => return Ok(Vc::cell(!side_effects)),
            SideEffectsValue::Glob(glob) => {
                if let Some(rel_path) = package_json
                    .parent()
                    .await?
                    .get_relative_path_to(&*path.await?)
                {
                    return Ok(Vc::cell(!glob.await?.execute(&rel_path)));
                }
            }
        }
    }

    Ok(Vc::cell(false))
}

#[turbo_tasks::value(transparent)]
pub struct EcmascriptChunkPlaceables(Vec<Vc<Box<dyn EcmascriptChunkPlaceable>>>);

#[turbo_tasks::value_impl]
impl EcmascriptChunkPlaceables {
    #[turbo_tasks::function]
    pub fn empty() -> Vc<Self> {
        Vc::cell(Vec::new())
    }
}

#[turbo_tasks::value(shared)]
pub enum EcmascriptExports {
    EsmExports(Vc<EsmExports>),
    DynamicNamespace,
    CommonJs,
    Value,
    None,
}

#[turbo_tasks::value_impl]
impl EcmascriptExports {
    #[turbo_tasks::function]
    pub async fn needs_facade(&self) -> Result<Vc<bool>> {
        Ok(match self {
            EcmascriptExports::EsmExports(exports) => {
                let exports = exports.await?;
                let has_reexports = !exports.star_exports.is_empty()
                    || exports.exports.iter().any(|(_, export)| {
                        matches!(
                            export,
                            EsmExport::ImportedBinding(..) | EsmExport::ImportedNamespace(_)
                        )
                    });
                Vc::cell(has_reexports)
            }
            _ => Vc::cell(false),
        })
    }
}
