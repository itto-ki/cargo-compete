use crate::shell::Shell;
use anyhow::{bail, Context as _};
use easy_ext::ext;
use indexmap::IndexMap;
use itertools::Itertools as _;
use krates::cm;
use serde::Deserialize;
use std::{
    path::{Path, PathBuf},
    str,
};
use url::Url;

#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct PackageMetadataCargoCompete {
    pub(crate) config: PathBuf,
    pub(crate) bin: IndexMap<String, PackageMetadataCargoCompeteBin>,
}

impl PackageMetadataCargoCompete {
    pub(crate) fn bin_by_bin_index(
        &self,
        bin_index: impl AsRef<str>,
    ) -> anyhow::Result<&PackageMetadataCargoCompeteBin> {
        let bin_index = bin_index.as_ref();

        self.bin.get(bin_index).with_context(|| {
            format!(
                "could not find `{}` in `package.metadata.cargo-compete.bin`",
                bin_index,
            )
        })
    }

    pub(crate) fn bin_by_bin_name(
        &self,
        bin_name: impl AsRef<str>,
    ) -> anyhow::Result<&PackageMetadataCargoCompeteBin> {
        let bin_name = bin_name.as_ref();

        self.bin
            .values()
            .find(|PackageMetadataCargoCompeteBin { name, .. }| name == bin_name)
            .with_context(|| {
                format!(
                    "could not find metadata in `package.metadata.cargo-compete.bin` which points \
                     `{}`",
                    bin_name,
                )
            })
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct PackageMetadataCargoCompeteBin {
    pub(crate) name: String,
    pub(crate) problem: TargetProblem,
}

#[derive(Deserialize, Debug, Ord, PartialOrd, Eq, PartialEq)]
#[serde(rename_all = "kebab-case", tag = "platform")]
pub(crate) enum TargetProblem {
    Atcoder {
        contest: String,
        index: String,
        url: Option<Url>,
    },
    Codeforces {
        contest: String,
        index: String,
        url: Option<Url>,
    },
    Yukicoder(TargetProblemYukicoder),
}

impl TargetProblem {
    pub(crate) fn url(&self) -> Option<&Url> {
        match self {
            Self::Atcoder { url, .. }
            | Self::Codeforces { url, .. }
            | Self::Yukicoder(TargetProblemYukicoder::Problem { url, .. })
            | Self::Yukicoder(TargetProblemYukicoder::Contest { url, .. }) => url.as_ref(),
        }
    }
}

#[derive(Deserialize, Debug, Ord, PartialOrd, Eq, PartialEq)]
#[serde(rename_all = "kebab-case", tag = "kind")]
pub(crate) enum TargetProblemYukicoder {
    Problem {
        no: u64,
        url: Option<Url>,
    },
    Contest {
        contest: String,
        index: String,
        url: Option<Url>,
    },
}

#[ext(MetadataExt)]
impl cm::Metadata {
    pub(crate) fn all_members(&self) -> Vec<&cm::Package> {
        self.packages
            .iter()
            .filter(|cm::Package { id, .. }| self.workspace_members.contains(id))
            .collect()
    }

    pub(crate) fn query_for_member<'a, S: AsRef<str>>(
        &'a self,
        spec: Option<S>,
    ) -> anyhow::Result<&'a cm::Package> {
        if let Some(spec_str) = spec {
            let spec_str = spec_str.as_ref();
            let spec = spec_str.parse::<krates::PkgSpec>()?;

            match *self
                .packages
                .iter()
                .filter(|package| {
                    self.workspace_members.contains(&package.id) && spec.matches(package)
                })
                .collect::<Vec<_>>()
            {
                [] => bail!("package `{}` is not a member of the workspace", spec_str),
                [member] => Ok(member),
                [_, _, ..] => bail!("`{}` matched multiple members?????", spec_str),
            }
        } else {
            let current_member = self
                .resolve
                .as_ref()
                .and_then(|cm::Resolve { root, .. }| root.as_ref())
                .map(|root| &self[root]);

            if let Some(current_member) = current_member {
                Ok(current_member)
            } else {
                match *self.workspace_members.iter().collect::<Vec<_>>() {
                    [] => bail!("this workspace has no members",),
                    [one] => Ok(&self[one]),
                    [..] => {
                        bail!(
                            "this manifest is virtual, and the workspace has {} members. specify \
                             one with `--manifest-path` or `--package`",
                            self.workspace_members.len(),
                        );
                    }
                }
            }
        }
    }
}

#[ext(PackageExt)]
impl cm::Package {
    pub(crate) fn manifest_dir(&self) -> &Path {
        self.manifest_path
            .parent()
            .expect("`manifest_path` should end with `Cargo.toml`")
    }

    pub(crate) fn manifest_dir_utf8(&self) -> &str {
        self.manifest_dir()
            .to_str()
            .expect("this is from JSON string")
    }

    pub(crate) fn read_package_metadata(&self) -> anyhow::Result<PackageMetadataCargoCompete> {
        let CargoToml {
            package:
                CargoTomlPackage {
                    metadata: CargoTomlPackageMetadata { cargo_compete },
                },
        } = crate::fs::read_toml(&self.manifest_path)?;
        return Ok(cargo_compete);

        #[derive(Deserialize)]
        #[serde(rename_all = "kebab-case")]
        struct CargoToml {
            package: CargoTomlPackage,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "kebab-case")]
        struct CargoTomlPackage {
            metadata: CargoTomlPackageMetadata,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "kebab-case")]
        struct CargoTomlPackageMetadata {
            cargo_compete: PackageMetadataCargoCompete,
        }
    }

    pub(crate) fn bin_target_by_name(&self, name: impl AsRef<str>) -> anyhow::Result<&cm::Target> {
        let name = name.as_ref();

        self.targets
            .iter()
            .find(|t| t.name == name && t.kind == ["bin".to_owned()])
            .with_context(|| format!("no bin target named `{}` in `{}`", name, self.name))
    }

    pub(crate) fn bin_target_by_src_path(
        &self,
        src_path: impl AsRef<Path>,
    ) -> anyhow::Result<&cm::Target> {
        let src_path = src_path.as_ref();

        self.targets
            .iter()
            .find(|t| t.src_path == src_path && t.kind == ["bin".to_owned()])
            .with_context(|| {
                format!(
                    "no bin target which `src_path` is `{}` in `{}`",
                    src_path.display(),
                    self.name,
                )
            })
    }

    pub(crate) fn all_bin_targets_sorted(&self) -> Vec<&cm::Target> {
        self.targets
            .iter()
            .filter(|cm::Target { kind, .. }| *kind == ["bin".to_owned()])
            .sorted_by(|t1, t2| t1.name.cmp(&t2.name))
            .collect()
    }
}

pub(crate) fn locate_project(cwd: impl AsRef<Path>) -> anyhow::Result<PathBuf> {
    let cwd = cwd.as_ref();

    cwd.ancestors()
        .map(|p| p.join("Cargo.toml"))
        .find(|p| p.exists())
        .with_context(|| {
            format!(
                "could not find `Cargo.toml` in `{}` or any parent directory. first, run \
                 `cargo compete init` and `cd` to a workspace",
                cwd.display(),
            )
        })
}

pub(crate) fn cargo_metadata(
    manifest_path: impl AsRef<Path>,
    cwd: impl AsRef<Path>,
) -> cm::Result<cm::Metadata> {
    cm::MetadataCommand::new()
        .manifest_path(manifest_path.as_ref())
        .current_dir(cwd.as_ref())
        .exec()
}

pub(crate) fn cargo_metadata_no_deps(
    manifest_path: impl AsRef<Path>,
    cwd: impl AsRef<Path>,
) -> cm::Result<cm::Metadata> {
    cm::MetadataCommand::new()
        .manifest_path(manifest_path.as_ref())
        .no_deps()
        .current_dir(cwd.as_ref())
        .exec()
}

pub(crate) fn set_cargo_config_build_target_dir(
    dir: &Path,
    shell: &mut Shell,
) -> anyhow::Result<()> {
    crate::fs::create_dir_all(dir.join(".cargo"))?;

    let cargo_config_path = dir.join(".cargo").join("config.toml");

    let mut cargo_config = if cargo_config_path.exists() {
        crate::fs::read_to_string(&cargo_config_path)?
    } else {
        r#"[build]
"#
        .to_owned()
    }
    .parse::<toml_edit::Document>()
    .with_context(|| {
        format!(
            "could not parse the TOML file at `{}`",
            cargo_config_path.display(),
        )
    })?;

    if cargo_config["build"]["target-dir"].is_none() {
        if cargo_config["build"].is_none() {
            let mut tbl = toml_edit::Table::new();
            tbl.set_implicit(true);
            cargo_config["build"] = toml_edit::Item::Table(tbl);
        }

        cargo_config["build"]["target-dir"] = toml_edit::value("target");

        crate::fs::write(
            &cargo_config_path,
            cargo_config.to_string_in_original_order(),
        )?;

        shell.status("Wrote", cargo_config_path.display())?;
    }
    Ok(())
}
