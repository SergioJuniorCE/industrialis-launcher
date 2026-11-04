//! Modrinth catalog for the instance's own Minecraft version and mod loader.
//!
//! Projects and versions are validated before they are shown or downloaded.
//! Installation uses the shared content transaction, so a failed dependency or
//! download never leaves a partially installed mod set behind.

use crate::{
    content::{self, ContentKind, SourceRecord},
    game_target::{self, GameTarget},
    paths::LauncherPaths,
};
use anyhow::{ensure, Context};
use async_trait::async_trait;
use serde::Deserialize;
use sha2::{Digest, Sha512};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    time::Duration,
};

pub const SITE: &str = "https://modrinth.com/mod";
const API: &str = "https://api.modrinth.com/v2";
const PAGE_SIZE: usize = 20;
const MAX_JSON: u64 = 8 * 1024 * 1024;
const MAX_FILE: u64 = 128 * 1024 * 1024;
const MAX_DEPENDENCY_DEPTH: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub downloads: u64,
    pub icon_url: Option<String>,
}
impl Project {
    pub fn page(&self) -> String {
        let slug = if self.slug.is_empty() {
            &self.id
        } else {
            &self.slug
        };
        format!("{SITE}/{slug}")
    }
    fn validate(&self) -> anyhow::Result<()> {
        ensure!(valid_id(&self.id), "Invalid Modrinth project identity");
        ensure!(
            !self.name.trim().is_empty() && self.name.len() <= 256,
            "Modrinth project has an invalid name"
        );
        ensure!(
            self.slug.is_empty()
                || (self.slug.len() <= 128
                    && self
                        .slug
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')),
            "Invalid Modrinth project slug"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dependency {
    pub project_id: Option<String>,
    pub version_id: Option<String>,
    pub kind: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    pub project_id: String,
    pub id: String,
    pub name: String,
    pub version_number: String,
    pub version_type: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub date_published: String,
    pub filename: String,
    pub url: String,
    pub size: u64,
    pub sha512: String,
    pub sha1: String,
    pub dependencies: Vec<Dependency>,
}
impl Version {
    pub fn is_compatible(&self, target: &GameTarget) -> bool {
        self.game_versions
            .iter()
            .any(|version| version == &target.game_version)
            && self.loaders.iter().any(|loader| {
                target
                    .modrinth_loaders()
                    .any(|supported| loader == supported)
            })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchPage {
    pub projects: Vec<Project>,
    pub total: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub project: Project,
    pub version: Version,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanItem {
    pub project: Project,
    pub version: Version,
    pub required_by: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstallPlan {
    pub target: GameTarget,
    pub items: Vec<PlanItem>,
    pub skipped: Vec<String>,
    pub conflicts: Vec<String>,
}

#[derive(Deserialize)]
struct RawSearch {
    hits: Vec<RawHit>,
    #[serde(default)]
    total_hits: u64,
}
#[derive(Deserialize)]
struct RawHit {
    project_id: String,
    #[serde(default)]
    slug: String,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    icon_url: Option<String>,
}
#[derive(Deserialize)]
struct RawProject {
    id: String,
    #[serde(default)]
    slug: String,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    icon_url: Option<String>,
}
#[derive(Deserialize)]
struct RawVersion {
    id: String,
    project_id: String,
    name: String,
    version_number: String,
    version_type: String,
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    loaders: Vec<String>,
    #[serde(default)]
    date_published: String,
    #[serde(default)]
    dependencies: Vec<RawDependency>,
    files: Vec<RawFile>,
}
#[derive(Deserialize)]
struct RawDependency {
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    version_id: Option<String>,
    #[serde(default)]
    dependency_type: String,
}
#[derive(Deserialize)]
struct RawFile {
    url: String,
    filename: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    primary: bool,
    #[serde(default)]
    hashes: RawHashes,
}
#[derive(Default, Deserialize)]
struct RawHashes {
    #[serde(default)]
    sha512: Option<String>,
    #[serde(default)]
    sha1: Option<String>,
}

const API_HOSTS: &[&str] = &["api.modrinth.com"];
const CDN_HOSTS: &[&str] = &["cdn.modrinth.com"];

fn client(hosts: &'static [&'static str]) -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent("IndustrialisLauncher/Modrinth")
        .timeout(Duration::from_secs(180))
        .connect_timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::custom(move |attempt| {
            let url = attempt.url();
            if attempt.previous().len() >= 5
                || url.scheme() != "https"
                || !hosts.contains(&url.host_str().unwrap_or_default())
                || url.port().is_some()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.fragment().is_some()
            {
                attempt.error("Unexpected Modrinth redirect")
            } else {
                attempt.follow()
            }
        }))
        .build()?)
}

async fn bounded(mut response: reqwest::Response, limit: u64) -> anyhow::Result<Vec<u8>> {
    ensure!(
        response.status().is_success(),
        "Modrinth returned HTTP {}. Try again later.",
        response.status()
    );
    ensure!(
        response.content_length().is_none_or(|size| size <= limit),
        "Modrinth response exceeds its size limit"
    );
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(
            bytes.len() as u64 + chunk.len() as u64 <= limit,
            "Modrinth response exceeds its size limit"
        );
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub async fn search(target: &GameTarget, query: &str, offset: usize) -> anyhow::Result<SearchPage> {
    target.validate()?;
    let query = query.trim();
    ensure!(
        query.len() <= 128 && offset <= 1000,
        "Modrinth search is too long or exceeds the page limit"
    );
    let loaders = target
        .modrinth_loaders()
        .map(|loader| format!(r#""categories:{loader}""#))
        .collect::<Vec<_>>()
        .join(",");
    let facets = format!(
        r#"[["project_type:mod"],["versions:{}"],[{loaders}]]"#,
        target.game_version
    );
    let params = [
        ("query".to_owned(), query.to_owned()),
        ("limit".to_owned(), PAGE_SIZE.to_string()),
        ("offset".to_owned(), offset.to_string()),
        (
            "index".to_owned(),
            if query.is_empty() {
                "downloads"
            } else {
                "relevance"
            }
            .to_owned(),
        ),
        ("facets".to_owned(), facets),
    ];
    let raw = bounded(
        client(API_HOSTS)?
            .get(format!("{API}/search"))
            .query(&params)
            .send()
            .await?,
        MAX_JSON,
    )
    .await?;
    parse_search(&raw)
}

pub async fn project(project_id: &str) -> anyhow::Result<Project> {
    ensure!(valid_id(project_id), "Invalid Modrinth project identity");
    let raw = bounded(
        client(API_HOSTS)?
            .get(format!("{API}/project/{project_id}"))
            .send()
            .await?,
        MAX_JSON,
    )
    .await?;
    let project = parse_project(&raw)?;
    ensure!(
        project.id == project_id,
        "Modrinth returned a different project"
    );
    Ok(project)
}

pub async fn versions(target: &GameTarget, project_id: &str) -> anyhow::Result<Vec<Version>> {
    target.validate()?;
    ensure!(valid_id(project_id), "Invalid Modrinth project identity");
    let loaders = target
        .modrinth_loaders()
        .map(|loader| format!(r#""{loader}""#))
        .collect::<Vec<_>>()
        .join(",");
    let params = [
        ("game_versions", format!(r#"["{}"]"#, target.game_version)),
        ("loaders", format!("[{loaders}]")),
    ];
    let raw = bounded(
        client(API_HOSTS)?
            .get(format!("{API}/project/{project_id}/version"))
            .query(&params)
            .send()
            .await?,
        MAX_JSON,
    )
    .await?;
    parse_versions(target, &raw)
}

async fn version(target: &GameTarget, version_id: &str) -> anyhow::Result<Version> {
    target.validate()?;
    ensure!(valid_id(version_id), "Invalid Modrinth version identity");
    let raw = bounded(
        client(API_HOSTS)?
            .get(format!("{API}/version/{version_id}"))
            .send()
            .await?,
        MAX_JSON,
    )
    .await?;
    let version = parse_version(target, &raw)?;
    ensure!(
        version.id == version_id,
        "Modrinth returned a different version"
    );
    Ok(version)
}

pub async fn download(version: &Version) -> anyhow::Result<Vec<u8>> {
    validate_version_identity(version)?;
    let raw = bounded(
        client(CDN_HOSTS)?.get(&version.url).send().await?,
        version.size.max(1),
    )
    .await?;
    verify_download(version, &raw)?;
    Ok(raw)
}

pub fn verify_download(version: &Version, raw: &[u8]) -> anyhow::Result<()> {
    ensure!(
        raw.len() as u64 == version.size,
        "Download size mismatch; nothing was installed"
    );
    if !version.sha512.is_empty() {
        ensure!(
            format!("{:x}", Sha512::digest(raw)).eq_ignore_ascii_case(&version.sha512),
            "Download checksum mismatch; nothing was installed"
        );
    } else {
        use sha1::Digest as _;
        let digest = sha1::Sha1::digest(raw);
        let computed: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
        ensure!(
            computed.eq_ignore_ascii_case(&version.sha1),
            "Download checksum mismatch; nothing was installed"
        );
    }
    Ok(())
}

#[async_trait]
trait Resolver: Sync {
    async fn dependency_version(
        &self,
        parent: &str,
        dep: &Dependency,
    ) -> anyhow::Result<Option<Version>>;
    async fn project(&self, project_id: &str) -> anyhow::Result<Project>;
    async fn version(&self, version_id: &str) -> anyhow::Result<Version>;
}

struct HttpResolver<'a> {
    target: &'a GameTarget,
}

#[async_trait]
impl Resolver for HttpResolver<'_> {
    async fn dependency_version(
        &self,
        parent: &str,
        dep: &Dependency,
    ) -> anyhow::Result<Option<Version>> {
        dependency_version(self.target, parent, dep).await
    }
    async fn project(&self, project_id: &str) -> anyhow::Result<Project> {
        project(project_id).await
    }
    async fn version(&self, version_id: &str) -> anyhow::Result<Version> {
        version(self.target, version_id).await
    }
}

async fn dependency_version(
    target: &GameTarget,
    parent: &str,
    dep: &Dependency,
) -> anyhow::Result<Option<Version>> {
    if let Some(version_id) = &dep.version_id {
        let resolved = version(target, version_id)
            .await
            .with_context(|| format!("Could not load a dependency of {parent}"))?;
        ensure!(
            dep.project_id
                .as_ref()
                .is_none_or(|project| project == &resolved.project_id),
            "Modrinth returned a different project for a dependency of {parent}"
        );
        ensure!(
            resolved.is_compatible(target),
            "A dependency of {parent} has no {} file",
            target.summary()
        );
        return Ok(Some(resolved));
    }
    let Some(project_id) = &dep.project_id else {
        return Ok(None);
    };
    let resolved = versions(target, project_id)
        .await
        .with_context(|| format!("Could not resolve a dependency of {parent}"))?;
    ensure!(
        !resolved.is_empty(),
        "A required dependency of {parent} has no {} file",
        target.summary()
    );
    ensure!(
        resolved
            .iter()
            .all(|version| version.project_id == *project_id),
        "Modrinth returned a different project for a dependency of {parent}"
    );
    Ok(preferred_version(resolved))
}

fn preferred_version(mut versions: Vec<Version>) -> Option<Version> {
    versions.sort_by(|a, b| b.date_published.cmp(&a.date_published));
    // Prefer a stable release; only fall back to beta/alpha when necessary.
    let preferred = versions
        .iter()
        .position(|version| version.version_type == "release")
        .unwrap_or(0);
    versions.into_iter().nth(preferred)
}

/// Build a reviewable install plan from explicit selections.
pub async fn plan(
    paths: &LauncherPaths,
    id: &str,
    selections: Vec<Selection>,
) -> anyhow::Result<InstallPlan> {
    ensure!(!selections.is_empty(), "Select at least one mod");
    let target = game_target::load(&paths.instance_dir(id)?).await?;
    let entries = content::list(paths, id, ContentKind::Mod).await?;
    let installed_projects: HashSet<String> = entries
        .iter()
        .filter_map(|entry| entry.source.as_ref()?.project_id.clone())
        .collect();
    let installed_names: Vec<String> = entries.iter().map(|entry| entry.filename.clone()).collect();
    plan_with(
        &HttpResolver { target: &target },
        target.clone(),
        selections,
        &installed_projects,
        &installed_names,
    )
    .await
}

fn note(plan: &mut InstallPlan, noted: &mut HashSet<String>, message: String) {
    if noted.insert(message.clone()) {
        plan.skipped.push(message);
    }
}

fn conflict(plan: &mut InstallPlan, message: String) {
    if !plan.conflicts.contains(&message) {
        plan.conflicts.push(message);
    }
}

async fn plan_with<R: Resolver>(
    resolver: &R,
    target: GameTarget,
    selections: Vec<Selection>,
    installed_projects: &HashSet<String>,
    installed_names: &[String],
) -> anyhow::Result<InstallPlan> {
    target.validate()?;
    let mut plan = InstallPlan {
        target: target.clone(),
        items: Vec::new(),
        skipped: Vec::new(),
        conflicts: Vec::new(),
    };
    let mut planned: HashMap<String, usize> = HashMap::new();
    let mut queued = HashSet::new();
    let mut required_by: HashMap<String, Vec<String>> = HashMap::new();
    let mut noted = HashSet::new();
    let mut plan_projects = HashSet::new();
    let mut plan_identities = HashSet::new();
    let mut total_bytes = 0u64;
    let mut queue: VecDeque<(Project, Version, usize)> = VecDeque::new();

    for selection in selections {
        selection.project.validate()?;
        validate_version(&selection.version, &target)?;
        ensure!(
            selection.version.project_id == selection.project.id,
            "The selected file does not belong to this Modrinth project"
        );
        ensure!(
            selection.version.is_compatible(&target),
            "{} has no {} file",
            selection.project.name,
            target.summary()
        );
        if installed_projects.contains(&selection.project.id)
            || filename_collision(installed_names, &selection.version.filename)
        {
            note(
                &mut plan,
                &mut noted,
                format!("{} is already installed", selection.project.name),
            );
            continue;
        }
        if queued.insert(selection.project.id.clone()) {
            queue.push_back((selection.project, selection.version, 0));
        }
    }

    while let Some((item_project, version, depth)) = queue.pop_front() {
        ensure!(
            depth <= MAX_DEPENDENCY_DEPTH,
            "The dependency chain for {} is too deep",
            item_project.name
        );
        if planned.contains_key(&item_project.id) {
            continue;
        }
        if installed_projects.contains(&item_project.id)
            || filename_collision(installed_names, &version.filename)
        {
            note(
                &mut plan,
                &mut noted,
                format!("{} is already installed", item_project.name),
            );
            continue;
        }
        ensure!(
            plan.items.len() < content::MAX_INSTALL_ITEMS,
            "The install plan has too many files"
        );
        ensure!(
            plan_projects.insert(item_project.id.clone()),
            "{} is selected more than once",
            item_project.name
        );
        ensure!(
            plan_identities.insert(crate::pack_files::mod_identity_from_filename(
                &version.filename
            )),
            "{} conflicts with another selected mod",
            item_project.name
        );
        total_bytes = total_bytes
            .checked_add(version.size)
            .context("The install plan size overflowed")?;
        ensure!(
            total_bytes <= content::MAX_INSTALL_BYTES,
            "The install plan exceeds the 512 MB limit"
        );
        let index = plan.items.len();
        let mut parents = required_by.remove(&item_project.id).unwrap_or_default();
        parents.sort();
        parents.dedup();
        plan.items.push(PlanItem {
            project: item_project.clone(),
            version: version.clone(),
            required_by: parents,
        });
        planned.insert(item_project.id.clone(), index);

        for dependency in &version.dependencies {
            match dependency.kind.as_str() {
                "required" => {
                    let Some(dependency_version) = resolver
                        .dependency_version(&version.name, dependency)
                        .await?
                    else {
                        anyhow::bail!(
                            "A required dependency of {} has no Modrinth project",
                            version.name
                        );
                    };
                    if dependency_version.project_id == version.project_id {
                        continue;
                    }
                    if let Some(&index) = planned.get(&dependency_version.project_id) {
                        if !plan.items[index].required_by.contains(&version.name) {
                            plan.items[index].required_by.push(version.name.clone());
                            plan.items[index].required_by.sort();
                        }
                        continue;
                    }
                    if queued.contains(&dependency_version.project_id) {
                        required_by
                            .entry(dependency_version.project_id.clone())
                            .or_default()
                            .push(version.name.clone());
                        continue;
                    }
                    let dependency_project = resolver
                        .project(&dependency_version.project_id)
                        .await
                        .with_context(|| {
                            format!("Could not load a dependency of {}", version.name)
                        })?;
                    ensure!(
                        dependency_project.id == dependency_version.project_id,
                        "Modrinth returned a different project for a dependency"
                    );
                    if installed_projects.contains(&dependency_project.id)
                        || filename_collision(installed_names, &dependency_version.filename)
                    {
                        note(
                            &mut plan,
                            &mut noted,
                            format!("{} is already installed", dependency_project.name),
                        );
                        continue;
                    }
                    required_by
                        .entry(dependency_project.id.clone())
                        .or_default()
                        .push(version.name.clone());
                    if queued.insert(dependency_project.id.clone()) {
                        queue.push_back((dependency_project, dependency_version, depth + 1));
                    }
                }
                "incompatible" => {
                    let target = match (&dependency.project_id, &dependency.version_id) {
                        (Some(project_id), _) => Some(project_id.clone()),
                        (None, Some(version_id)) => {
                            Some(resolver.version(version_id).await?.project_id)
                        }
                        (None, None) => None,
                    };
                    let Some(target) = target else { continue };
                    if target == version.project_id {
                        continue;
                    }
                    let target_name = queue
                        .iter()
                        .find(|(project, _, _)| project.id == target)
                        .map(|(project, _, _)| project.name.clone())
                        .or_else(|| {
                            planned
                                .get(&target)
                                .map(|index| plan.items[*index].project.name.clone())
                        })
                        .unwrap_or_else(|| target.clone());
                    if planned.contains_key(&target)
                        || queued.contains(&target)
                        || installed_projects.contains(&target)
                    {
                        conflict(
                            &mut plan,
                            format!("{} is marked incompatible with {target_name}", version.name),
                        );
                    }
                }
                _ => {}
            }
        }
    }

    Ok(plan)
}

fn filename_collision(installed: &[String], filename: &str) -> bool {
    let identity = crate::pack_files::mod_identity_from_filename(filename);
    installed
        .iter()
        .any(|existing| crate::pack_files::mod_identity_from_filename(existing) == identity)
}

/// Download and verify every file first, then install the whole set in one
/// content transaction. A failure installs nothing.
pub async fn install(paths: &LauncherPaths, id: &str, plan: &InstallPlan) -> anyhow::Result<usize> {
    plan.target.validate()?;
    let current = game_target::load(&paths.instance_dir(id)?).await?;
    ensure!(
        current == plan.target,
        "The instance Minecraft version or mod loader changed. Review the install list again."
    );
    ensure!(!plan.items.is_empty(), "No mods to install");
    ensure!(
        plan.conflicts.is_empty(),
        "Remove the conflicting mods before installing"
    );
    ensure!(
        plan.items.len() <= content::MAX_INSTALL_ITEMS,
        "The install plan has too many files"
    );
    let total = plan
        .items
        .iter()
        .try_fold(0u64, |sum, item| sum.checked_add(item.version.size))
        .context("The install plan size overflowed")?;
    ensure!(
        total <= content::MAX_INSTALL_BYTES,
        "The install plan exceeds the 512 MB limit"
    );
    let mut projects = HashSet::new();
    let mut identities = HashSet::new();
    for item in &plan.items {
        item.project.validate()?;
        ensure!(
            item.project.id == item.version.project_id,
            "The selected file does not belong to this Modrinth project"
        );
        validate_version(&item.version, &plan.target)?;
        ensure!(
            projects.insert(item.project.id.clone()),
            "The same project is selected more than once"
        );
        ensure!(
            identities.insert(crate::pack_files::mod_identity_from_filename(
                &item.version.filename
            )),
            "{} conflicts with another selected mod",
            item.version.filename
        );
    }
    ensure!(
        !crate::launch::is_already_running(paths, id).await?,
        "Stop the game before changing add-ons"
    );
    let mut pending = Vec::with_capacity(plan.items.len());
    for item in &plan.items {
        let raw = download(&item.version)
            .await
            .with_context(|| format!("Could not download {}", item.project.name))?;
        let archive_sha256 = content::digest(&raw);
        pending.push(content::PendingContent {
            name: item.version.filename.clone(),
            raw,
            source: SourceRecord {
                project: item.project.page(),
                release: item.version.version_number.clone(),
                url: item.version.url.clone(),
                archive_sha256,
                pack_series: None,
                provider: Some("modrinth".into()),
                project_id: Some(item.project.id.clone()),
                version_id: Some(item.version.id.clone()),
            },
        });
    }
    content::install_batch(paths, id, ContentKind::Mod, pending).await
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn valid_filename(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    !name.is_empty()
        && name.len() <= 200
        && lower.ends_with(".jar")
        && !name.contains('/')
        && !name.contains('\\')
        && crate::minecraft_files::sanitize_rel_path(name)
            .ok()
            .as_deref()
            == Some(name)
}

fn valid_hash(value: &str, length: usize) -> bool {
    value.len() == length && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_version(version: &Version, target: &GameTarget) -> anyhow::Result<()> {
    validate_version_identity(version)?;
    ensure!(
        version.is_compatible(target),
        "{} has no {} file",
        version.name,
        target.summary()
    );
    Ok(())
}

fn validate_version_identity(version: &Version) -> anyhow::Result<()> {
    ensure!(
        valid_id(&version.project_id) && valid_id(&version.id),
        "Invalid Modrinth version identity"
    );
    ensure!(
        valid_filename(&version.filename),
        "Modrinth returned an unsafe mod filename"
    );
    ensure!(
        !version.name.is_empty()
            && version.name.len() <= 256
            && !version.version_number.is_empty()
            && version.version_number.len() <= 128,
        "Modrinth returned an invalid version label"
    );
    ensure!(
        version.size > 0 && version.size <= MAX_FILE,
        "Modrinth file exceeds the download limit"
    );
    ensure!(
        version.sha512.is_empty() || valid_hash(&version.sha512, 128),
        "Invalid Modrinth SHA-512"
    );
    ensure!(
        version.sha1.is_empty() || valid_hash(&version.sha1, 40),
        "Invalid Modrinth SHA-1"
    );
    ensure!(
        !version.sha512.is_empty() || !version.sha1.is_empty(),
        "Modrinth did not provide a checksum"
    );
    let url = reqwest::Url::parse(&version.url).context("Invalid Modrinth download URL")?;
    ensure!(
        url.scheme() == "https"
            && url.host_str() == Some("cdn.modrinth.com")
            && url.port().is_none()
            && url.username().is_empty()
            && url.password().is_none()
            && url.fragment().is_none(),
        "Unexpected Modrinth download source"
    );
    ensure!(
        url.path().starts_with(&format!(
            "/data/{}/versions/{}/",
            version.project_id, version.id
        )),
        "Modrinth file does not belong to the selected version"
    );
    Ok(())
}

fn parse_search(raw: &[u8]) -> anyhow::Result<SearchPage> {
    let search: RawSearch =
        serde_json::from_slice(raw).context("Invalid Modrinth search response")?;
    ensure!(search.hits.len() <= PAGE_SIZE, "Invalid Modrinth page size");
    let mut projects = Vec::with_capacity(search.hits.len());
    for hit in search.hits {
        let project = Project {
            id: hit.project_id,
            slug: hit.slug,
            name: hit.title.trim().to_owned(),
            description: hit.description.trim().chars().take(600).collect(),
            downloads: hit.downloads,
            icon_url: hit.icon_url,
        };
        // One malformed catalog entry must not hide the rest of the page.
        if project.validate().is_ok() {
            projects.push(project);
        }
    }
    Ok(SearchPage {
        projects,
        total: usize::try_from(search.total_hits).unwrap_or(usize::MAX),
    })
}

fn parse_project(raw: &[u8]) -> anyhow::Result<Project> {
    let project: RawProject =
        serde_json::from_slice(raw).context("Invalid Modrinth project response")?;
    let project = Project {
        id: project.id,
        slug: project.slug,
        name: project.title.trim().to_owned(),
        description: project.description.trim().chars().take(600).collect(),
        downloads: project.downloads,
        icon_url: project.icon_url,
    };
    project.validate()?;
    Ok(project)
}

fn parse_version(target: &GameTarget, raw: &[u8]) -> anyhow::Result<Version> {
    let version: RawVersion =
        serde_json::from_slice(raw).context("Invalid Modrinth version response")?;
    let version = into_version(version)?;
    ensure!(
        version.is_compatible(target),
        "Modrinth returned a version for a different game or loader"
    );
    Ok(version)
}

fn parse_versions(target: &GameTarget, raw: &[u8]) -> anyhow::Result<Vec<Version>> {
    let versions: Vec<RawVersion> =
        serde_json::from_slice(raw).context("Invalid Modrinth versions response")?;
    let mut result = Vec::new();
    for version in versions {
        // Skip individual unusable entries instead of breaking the whole catalog.
        if let Ok(version) = into_version(version) {
            if version.is_compatible(target) {
                result.push(version);
            }
        }
    }
    result.sort_by(|a, b| b.date_published.cmp(&a.date_published));
    Ok(result)
}

fn into_version(raw: RawVersion) -> anyhow::Result<Version> {
    let file = raw
        .files
        .iter()
        .find(|file| file.primary)
        .or_else(|| raw.files.first())
        .context("Modrinth version has no downloadable file")?;
    let version = Version {
        project_id: raw.project_id,
        id: raw.id,
        name: raw.name.trim().to_owned(),
        version_number: raw.version_number.trim().to_owned(),
        version_type: raw.version_type.trim().to_ascii_lowercase(),
        game_versions: raw.game_versions,
        loaders: raw.loaders,
        date_published: raw.date_published,
        filename: file.filename.trim().to_owned(),
        url: file.url.clone(),
        size: file.size,
        sha512: file.hashes.sha512.clone().unwrap_or_default(),
        sha1: file.hashes.sha1.clone().unwrap_or_default(),
        dependencies: raw
            .dependencies
            .into_iter()
            .map(|dependency| Dependency {
                project_id: dependency.project_id,
                version_id: dependency.version_id,
                kind: dependency.dependency_type.trim().to_ascii_lowercase(),
            })
            .collect(),
    };
    ensure!(!version.name.is_empty(), "Modrinth version has no name");
    validate_version_identity(&version)?;
    Ok(version)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROJECT: &str = "GTNHMod";
    const VERSION: &str = "abc12345";

    fn file(primary: bool) -> serde_json::Value {
        serde_json::json!({
            "url": format!("https://cdn.modrinth.com/data/{PROJECT}/versions/{VERSION}/mod-1.0.jar"),
            "filename": "mod-1.0.jar",
            "size": 42,
            "primary": primary,
            "hashes": {"sha512": "a".repeat(128), "sha1": "b".repeat(40)}
        })
    }

    fn version_json() -> serde_json::Value {
        serde_json::json!({
            "id": VERSION,
            "project_id": PROJECT,
            "name": "Mod 1.0",
            "version_number": "1.0",
            "version_type": "release",
            "game_versions": ["1.7.10"],
            "loaders": ["forge"],
            "date_published": "2026-01-01T00:00:00Z",
            "dependencies": [{"project_id": "Dep", "version_id": null, "dependency_type": "required"}],
            "files": [file(true)]
        })
    }

    fn value<T: serde::de::DeserializeOwned>(raw: serde_json::Value) -> T {
        serde_json::from_value(raw).unwrap()
    }

    const TEST_GAME_VERSION: &str = "1.7.10";
    const TEST_LOADER: &str = "forge";

    fn target() -> GameTarget {
        GameTarget {
            game_version: TEST_GAME_VERSION.into(),
            loaders: vec![crate::game_target::ModLoader::Forge],
        }
    }

    #[test]
    fn search_and_primary_file_are_normalized_and_validated() {
        let search = parse_search(
            br#"{"hits":[{"project_id":"GTNHMod","slug":"mod","title":" Mod ","description":"desc","downloads":12}],"total_hits":1}"#,
        )
        .unwrap();
        assert_eq!(search.projects[0].name, "Mod");
        assert_eq!(search.projects[0].page(), "https://modrinth.com/mod/mod");

        let raw: RawVersion = value(version_json());
        let parsed = into_version(raw).unwrap();
        assert_eq!(parsed.filename, "mod-1.0.jar");
        assert!(parsed.is_compatible(&target()));
        assert_eq!(parsed.dependencies[0].kind, "required");
    }

    #[test]
    fn versions_must_match_the_instance_game_and_loader() {
        let raw: RawVersion = value(version_json());
        let mut version = into_version(raw).unwrap();
        assert!(validate_version(&version, &target()).is_ok());
        version.loaders = vec!["fabric".into()];
        assert!(validate_version(&version, &target()).is_err());
        version.loaders = vec!["forge".into()];
        version.game_versions = vec!["1.12.2".into()];
        assert!(validate_version(&version, &target()).is_err());
    }

    #[test]
    fn unsafe_or_foreign_downloads_are_rejected() {
        for (url, filename) in [
            ("https://example.com/mod.jar".to_owned(), "mod-1.0.jar"),
            (
                format!("https://cdn.modrinth.com/data/Other/versions/{VERSION}/mod-1.0.jar"),
                "mod-1.0.jar",
            ),
            (
                format!("http://cdn.modrinth.com/data/{PROJECT}/versions/{VERSION}/mod-1.0.jar"),
                "mod-1.0.jar",
            ),
            (
                format!("https://cdn.modrinth.com/data/{PROJECT}/versions/{VERSION}/mod-1.0.jar"),
                "../mod-1.0.jar",
            ),
        ] {
            let mut raw: RawVersion = value(version_json());
            raw.files[0].url = url;
            raw.files[0].filename = filename.into();
            assert!(into_version(raw).is_err());
        }
    }

    #[test]
    fn download_must_match_size_and_hash() {
        let raw: RawVersion = value(version_json());
        let mut version = into_version(raw).unwrap();
        version.size = 4;
        version.sha512 = format!("{:x}", Sha512::digest(b"data"));
        assert!(verify_download(&version, b"data").is_ok());
        assert!(verify_download(&version, b"date").is_err());
        version.size = 5;
        assert!(verify_download(&version, b"data").is_err());
    }

    struct Mock {
        projects: HashMap<String, Project>,
        versions: HashMap<String, Version>,
        exact: HashMap<String, Version>,
    }

    #[async_trait]
    impl Resolver for Mock {
        async fn dependency_version(
            &self,
            parent: &str,
            dep: &Dependency,
        ) -> anyhow::Result<Option<Version>> {
            if let Some(version_id) = &dep.version_id {
                return self
                    .exact
                    .get(version_id)
                    .cloned()
                    .with_context(|| format!("missing version {version_id} for {parent}"))
                    .map(Some);
            }
            if let Some(project_id) = &dep.project_id {
                return Ok(self.versions.get(project_id).cloned());
            }
            Ok(None)
        }
        async fn project(&self, project_id: &str) -> anyhow::Result<Project> {
            self.projects
                .get(project_id)
                .cloned()
                .with_context(|| format!("missing project {project_id}"))
        }
        async fn version(&self, version_id: &str) -> anyhow::Result<Version> {
            self.exact
                .get(version_id)
                .cloned()
                .with_context(|| format!("missing version {version_id}"))
        }
    }

    fn project(id: &str) -> Project {
        Project {
            id: id.into(),
            slug: id.to_ascii_lowercase(),
            name: format!("Mod {id}"),
            description: String::new(),
            downloads: 0,
            icon_url: None,
        }
    }

    fn version(project_id: &str, id: &str, dependencies: Vec<Dependency>) -> Version {
        let filename = format!("{}-{}.jar", project_id.to_ascii_lowercase(), id);
        Version {
            project_id: project_id.into(),
            id: id.into(),
            name: format!("Mod {project_id} {id}"),
            version_number: id.into(),
            version_type: "release".into(),
            game_versions: vec![TEST_GAME_VERSION.into()],
            loaders: vec![TEST_LOADER.into()],
            date_published: "2026-01-01T00:00:00Z".into(),
            url: format!("https://cdn.modrinth.com/data/{project_id}/versions/{id}/{filename}"),
            filename,
            size: 1,
            sha512: "a".repeat(128),
            sha1: "b".repeat(40),
            dependencies,
        }
    }

    fn required(project_id: &str) -> Dependency {
        Dependency {
            project_id: Some(project_id.into()),
            version_id: None,
            kind: "required".into(),
        }
    }

    fn required_version(version_id: &str) -> Dependency {
        Dependency {
            project_id: None,
            version_id: Some(version_id.into()),
            kind: "required".into(),
        }
    }

    fn mock(entries: Vec<(&str, Version)>) -> Mock {
        let mut projects = HashMap::new();
        let mut versions = HashMap::new();
        let mut exact = HashMap::new();
        for (id, version) in entries {
            projects.insert(id.into(), project(id));
            versions.insert(id.into(), version.clone());
            exact.insert(version.id.clone(), version);
        }
        Mock {
            projects,
            versions,
            exact,
        }
    }

    fn selection(entries: &Mock, id: &str) -> Selection {
        Selection {
            project: entries.projects[id].clone(),
            version: entries.versions[id].clone(),
        }
    }

    #[tokio::test]
    async fn resolves_required_dependencies_and_ignores_optional_ones() {
        let mut optional_c = required("C");
        optional_c.kind = "optional".into();
        let entries = mock(vec![
            ("A", version("A", "a1", vec![required("B"), optional_c])),
            ("B", version("B", "b1", vec![required_version("c1")])),
            ("C", version("C", "c1", vec![])),
        ]);
        let plan = plan_with(
            &entries,
            target(),
            vec![selection(&entries, "A")],
            &HashSet::new(),
            &[],
        )
        .await
        .unwrap();
        let names: Vec<_> = plan
            .items
            .iter()
            .map(|item| item.project.id.as_str())
            .collect();
        assert_eq!(names, vec!["A", "B", "C"]);
        assert_eq!(plan.items[1].required_by, vec!["Mod A a1"]);
        assert_eq!(plan.items[2].required_by, vec!["Mod B b1"]);
        assert!(plan.conflicts.is_empty());
    }

    #[tokio::test]
    async fn deduplicates_diamonds_and_survives_cycles() {
        let entries = mock(vec![
            ("A", version("A", "a1", vec![required("B"), required("C")])),
            ("B", version("B", "b1", vec![required("D")])),
            ("C", version("C", "c1", vec![required("D")])),
            ("D", version("D", "d1", vec![required("A")])),
        ]);
        let plan = plan_with(
            &entries,
            target(),
            vec![selection(&entries, "A")],
            &HashSet::new(),
            &[],
        )
        .await
        .unwrap();
        assert_eq!(plan.items.len(), 4);
        let d = plan
            .items
            .iter()
            .find(|item| item.project.id == "D")
            .unwrap();
        assert_eq!(d.required_by, vec!["Mod B b1", "Mod C c1"]);
    }

    #[tokio::test]
    async fn skips_installed_and_filename_collisions_once() {
        let entries = mock(vec![
            ("A", version("A", "a1", vec![required("D")])),
            ("B", version("B", "b1", vec![required("D"), required("C")])),
            ("C", version("C", "c1", vec![])),
            ("D", version("D", "d1", vec![])),
        ]);
        let installed_projects = HashSet::from(["D".to_string()]);
        let installed_names = vec!["c-c1.jar".to_string()];
        let plan = plan_with(
            &entries,
            target(),
            vec![selection(&entries, "A"), selection(&entries, "B")],
            &installed_projects,
            &installed_names,
        )
        .await
        .unwrap();
        assert_eq!(plan.items.len(), 2);
        assert_eq!(
            plan.skipped
                .iter()
                .filter(|note| note.contains("Mod D"))
                .count(),
            1
        );
        assert!(plan.skipped.iter().any(|note| note.contains("Mod C")));
    }

    #[tokio::test]
    async fn reports_incompatible_selections_and_installed_mods() {
        let mut incompatible = required("B");
        incompatible.kind = "incompatible".into();
        let entries = mock(vec![
            ("A", version("A", "a1", vec![incompatible])),
            ("B", version("B", "b1", vec![])),
        ]);
        let plan = plan_with(
            &entries,
            target(),
            vec![selection(&entries, "A"), selection(&entries, "B")],
            &HashSet::new(),
            &[],
        )
        .await
        .unwrap();
        assert_eq!(plan.conflicts.len(), 1);
        assert!(plan.conflicts[0].contains("Mod A"));
        assert!(plan.conflicts[0].contains("Mod B"));

        let installed = HashSet::from(["B".to_string()]);
        let plan = plan_with(
            &entries,
            target(),
            vec![selection(&entries, "A")],
            &installed,
            &[],
        )
        .await
        .unwrap();
        assert_eq!(plan.conflicts.len(), 1);
    }

    #[tokio::test]
    async fn required_dependency_without_identity_is_an_error() {
        let entries = mock(vec![(
            "A",
            version(
                "A",
                "a1",
                vec![Dependency {
                    project_id: None,
                    version_id: None,
                    kind: "required".into(),
                }],
            ),
        )]);
        assert!(plan_with(
            &entries,
            target(),
            vec![selection(&entries, "A")],
            &HashSet::new(),
            &[]
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn rejects_too_deep_too_many_and_oversized_plans() {
        let mut chain = Vec::new();
        for index in 0..18 {
            let deps = if index == 17 {
                Vec::new()
            } else {
                vec![required(&format!("P{}", index + 1))]
            };
            let id: &'static str = Box::leak(format!("P{index}").into_boxed_str());
            chain.push((id, version(id, &format!("v{index}"), deps)));
        }
        let entries = mock(chain);
        assert!(plan_with(
            &entries,
            target(),
            vec![selection(&entries, "P0")],
            &HashSet::new(),
            &[]
        )
        .await
        .is_err());

        let mut many = Vec::new();
        for index in 0..(content::MAX_INSTALL_ITEMS + 1) {
            let id: &'static str = Box::leak(format!("M{index}").into_boxed_str());
            many.push((id, version(id, &format!("v{index}"), Vec::new())));
        }
        let entries = mock(many);
        let selections: Vec<_> = (0..(content::MAX_INSTALL_ITEMS + 1))
            .map(|index| selection(&entries, &format!("M{index}")))
            .collect();
        assert!(
            plan_with(&entries, target(), selections, &HashSet::new(), &[])
                .await
                .is_err()
        );

        let mut huge = Vec::new();
        for index in 0..6 {
            let id: &'static str = Box::leak(format!("H{index}").into_boxed_str());
            let mut item = version(id, &format!("v{index}"), Vec::new());
            item.size = 100 * 1024 * 1024;
            huge.push((id, item));
        }
        let entries = mock(huge);
        let selections: Vec<_> = (0..6)
            .map(|index| selection(&entries, &format!("H{index}")))
            .collect();
        assert!(
            plan_with(&entries, target(), selections, &HashSet::new(), &[])
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn rejects_two_projects_shipping_the_same_mod_identity() {
        let mut second = version("B", "b1", vec![]);
        second.filename = "a-a1.jar".into();
        second.url = "https://cdn.modrinth.com/data/B/versions/b1/a-a1.jar".into();
        let entries = mock(vec![("A", version("A", "a1", vec![])), ("B", second)]);
        assert!(plan_with(
            &entries,
            target(),
            vec![selection(&entries, "A"), selection(&entries, "B")],
            &HashSet::new(),
            &[],
        )
        .await
        .is_err());
    }

    #[test]
    fn dependency_resolution_prefers_a_stable_release() {
        let mut alpha = version("A", "alpha", vec![]);
        alpha.version_type = "alpha".into();
        alpha.date_published = "2026-02-01T00:00:00Z".into();
        let mut beta = version("A", "beta", vec![]);
        beta.version_type = "beta".into();
        beta.date_published = "2026-03-01T00:00:00Z".into();
        let release = version("A", "release", vec![]);
        assert_eq!(
            preferred_version(vec![alpha.clone(), release.clone()])
                .unwrap()
                .id,
            "release"
        );
        assert_eq!(preferred_version(vec![alpha, beta]).unwrap().id, "beta");
    }
}
