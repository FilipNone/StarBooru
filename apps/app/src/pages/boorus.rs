//! The booru browser: Danbooru, Gelbooru, Moebooru, e621 and Blombooru sites
//! searched through the server, several at once, looked at like the gallery
//! and saved here with their tags, rating and source.
//!
//! One search is one request per site. A page of answers interleaves in the
//! order of the site chips, and a file two sites have shows once while
//! duplicates are hidden. What a search found stays while the app runs
//! (`KEPT`), so coming back from a post asks no site again.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::link::Link;
use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use futures_util::future::join_all;
use serde::Deserialize;
use starbooru_client::Client;
use starbooru_core::remote::clean_base;
use starbooru_core::saved::MAX_SEARCH_NAME_CHARS;
use starbooru_core::tag::{MAX_TAGS_PER_POST, TagError};
use starbooru_core::{
    BooruAccount, BooruApi, BooruSite, ImportJob, ImportRequest, JobState, MAX_SEARCH_SITES,
    RemotePost, RemoteSearch, RemoteSite, SITES, SaveBooruAccount, SaveRemoteSearch,
    SavedRemoteSearch, parse_tag_list,
};

use super::explore::Sides;
use super::gallery::{
    FIRST_NEAR, GRID_WIDTH, Grid, ViewMenu, WATCH_FAR, WATCH_FAR_OFF, WATCH_NEAR, WATCH_NEAR_OFF,
    WATCH_SCROLL, WATCH_SCROLL_OFF, WATCH_WIDTH, may_be_on_screen,
};
use super::{Choice, ErrorBanner, Select, Spinner};
use crate::Route;
use crate::activity::{self, JOBS};
use crate::icons::{Ico, Icon};
use crate::layout::Place;
use crate::store::{self, PREFS, Prefs, SavedBooru};

/// Sites added by their address that are remembered.
const MAX_SAVED: usize = 20;
/// Relayed pictures are behind signed addresses, good for six hours at least.
const MAX_KEPT_HOURS: i64 = 4;

/// The API a stored name stands for. `None` for one this version doesn't
/// know, which a newer one may have written.
pub fn api_of(name: &str) -> Option<BooruApi> {
    BooruApi::ALL.into_iter().find(|api| api.as_str() == name)
}

/// An address as the server keeps it.
fn tidy(base: &str) -> String {
    base.trim().trim_end_matches('/').to_owned()
}

/// The accounts an older version kept on this device, to move to the server.
fn kept_on_device(saved: &[SavedBooru]) -> Vec<SaveBooruAccount> {
    saved
        .iter()
        .filter(|s| !s.login.trim().is_empty() && !s.api_key.trim().is_empty())
        .filter_map(|s| {
            Some(SaveBooruAccount {
                api: api_of(&s.api)?,
                base: s.base.clone(),
                login: s.login.clone(),
                api_key: s.api_key.clone(),
            })
        })
        .collect()
}

/// Takes the account for `base` off this device.
fn forget_on_device(saved: &mut [SavedBooru], base: &str) {
    for site in saved.iter_mut().filter(|s| tidy(&s.base) == tidy(base)) {
        site.login.clear();
        site.api_key.clear();
    }
}

/// Moves the accounts an older version kept on this device to the server,
/// and takes each off the device once it is there. `save` says whether one
/// got there.
async fn move_accounts<Fut: Future<Output = bool>>(
    mut prefs: Signal<Prefs>,
    save: impl Fn(SaveBooruAccount) -> Fut,
) {
    // Read first and let go: each account that moves is written back to the
    // same preferences, and a `for` holds on to what it loops over.
    let kept = kept_on_device(&prefs.peek().boorus);
    for account in kept {
        let base = account.base.clone();
        if save(account).await {
            forget_on_device(&mut prefs.write().boorus, &base);
        }
    }
}

/// Adds `site` after the others, so its chip comes last. One already there
/// by that address stays where it is and only takes the new kind. The oldest
/// make room.
fn remember(saved: &mut Vec<SavedBooru>, site: SavedBooru) {
    match saved.iter_mut().find(|s| tidy(&s.base) == tidy(&site.base)) {
        Some(known) => known.api = site.api,
        None => saved.push(site),
    }
    let extra = saved.len().saturating_sub(MAX_SAVED);
    saved.drain(..extra);
}

/// A post's own tags with the user's extra ones first, so they survive the
/// per-post limit.
fn import_tags(post: &[String], extra: &str) -> Result<Vec<String>, TagError> {
    let mut tags = parse_tag_list(extra)?;
    for tag in post {
        if tags.len() >= MAX_TAGS_PER_POST {
            break;
        }
        if !tags.contains(tag) {
            tags.push(tag.clone());
        }
    }
    Ok(tags)
}

/// What saves `post` here: its file, with its page as the source. A post
/// whose site showed no file (Gelbooru without a key) is imported by its
/// page, and the server finds the file.
pub(super) fn import_request(post: &RemotePost, extra: &str) -> Result<ImportRequest, TagError> {
    let tags = import_tags(&post.import_tags(), extra)?;
    let by_page = post.file_url.trim().is_empty();
    Ok(ImportRequest {
        url: if by_page {
            post.page_url.clone()
        } else {
            post.file_url.clone()
        },
        tags,
        rating: post.rating,
        source: (!by_page).then(|| post.page_url.clone()),
        autotag: None,
    })
}

// ---------- addresses ----------

/// Text as it goes into one of this page's addresses. The router writes `&`,
/// `%` and `+` into a query's value as they are and reads them back as
/// something else, and a post's page on Gelbooru has `&` in it, as some tags
/// do. So those go in as `~` and their code in hex, which the router leaves
/// alone both ways.
pub(super) fn to_address(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '~' | '%' | '&' | '#' | '+' | ',' => out.push_str(&format!("~{:02X}", u32::from(c))),
            _ => out.push(c),
        }
    }
    out
}

/// The text an address carries (`to_address`). A `~` that isn't followed by
/// a code stays as it is.
pub(super) fn from_address(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('~') {
        let (before, after) = rest.split_at(at);
        out.push_str(before);
        let code = after
            .get(1..3)
            .filter(|hex| hex.bytes().all(|b| b.is_ascii_hexdigit()));
        match code
            .and_then(|hex| u8::from_str_radix(hex, 16).ok())
            .filter(u8::is_ascii)
        {
            Some(byte) => {
                out.push(char::from(byte));
                rest = after.get(3..).unwrap_or_default();
            }
            None => {
                out.push('~');
                rest = after.get(1..).unwrap_or_default();
            }
        }
    }
    out.push_str(rest);
    out
}

/// The grid's address for a search on the sites `s` names.
pub(super) fn search_route(q: &str, s: &str) -> Route {
    Route::Boorus {
        q: to_address(q),
        s: s.to_owned(),
    }
}

// ---------- sites ----------

/// A site the browser can search.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Site {
    pub name: String,
    pub site: RemoteSite,
    /// Added by its address, so it can be taken away again.
    pub custom: bool,
}

/// What stands for a preset in an address: its name in small letters.
fn preset_key(site: &BooruSite) -> String {
    site.name.to_lowercase()
}

/// The preset a site is, if it is one: the same kind at the same address.
fn preset_of(site: &RemoteSite) -> Option<BooruSite> {
    let base = tidy(&site.base);
    SITES
        .into_iter()
        .find(|s| s.api == site.api && s.base == base)
}

/// What stands for a site in an address: a preset's key, or `api|base`.
pub(super) fn site_key(site: &RemoteSite) -> String {
    match preset_of(site) {
        Some(preset) => preset_key(&preset),
        // A comma parts one site from the next: `to_address` takes it out.
        None => format!("{}|{}", site.api.as_str(), to_address(&tidy(&site.base))),
    }
}

/// The site a key stands for. `None` for one this version can't read.
fn site_of(key: &str) -> Option<RemoteSite> {
    let key = key.trim();
    let name = key.to_lowercase();
    if let Some(preset) = SITES.into_iter().find(|s| preset_key(s) == name) {
        return Some(RemoteSite {
            api: preset.api,
            base: preset.base.to_owned(),
        });
    }
    let (api, base) = key.split_once('|')?;
    Some(RemoteSite {
        api: api_of(api)?,
        base: clean_base(&from_address(base)).ok()?,
    })
}

/// The sites an address names (`s`), each once, as many as a search goes to.
pub(super) fn parse_sites(s: &str) -> Vec<RemoteSite> {
    let mut sites: Vec<RemoteSite> = Vec::new();
    for site in s.split(',').filter_map(site_of) {
        if sites.len() < MAX_SEARCH_SITES && !sites.contains(&site) {
            sites.push(site);
        }
    }
    sites
}

/// `sites` as an address names them.
pub(super) fn sites_param(sites: &[RemoteSite]) -> String {
    sites.iter().map(site_key).collect::<Vec<_>>().join(",")
}

/// The sites a search goes to: the address's, else the ones chosen last
/// time, else the first preset.
pub(super) fn chosen_sites(s: &str, last: &[String]) -> Vec<RemoteSite> {
    let some = |sites: Vec<RemoteSite>| (!sites.is_empty()).then_some(sites);
    some(parse_sites(s))
        .or_else(|| some(parse_sites(&last.join(","))))
        .unwrap_or_else(|| {
            SITES
                .first()
                .map(|site| RemoteSite {
                    api: site.api,
                    base: site.base.to_owned(),
                })
                .into_iter()
                .collect()
        })
}

/// The site whose tags complete what is typed in the search box: the first
/// of the chosen ones.
pub(super) fn completing_site(s: &str) -> Option<RemoteSite> {
    chosen_sites(s, &PREFS.peek().booru_sites)
        .into_iter()
        .next()
}

/// An address without its scheme, as a site's name when it has no other.
fn host_of(base: &str) -> String {
    let base = tidy(base);
    base.strip_prefix("https://")
        .or_else(|| base.strip_prefix("http://"))
        .unwrap_or(&base)
        .to_owned()
}

/// Every site there is a chip for: the presets, the ones the user added, and
/// any other the address names (a saved search's, say).
fn all_sites(added: &[SavedBooru], chosen: &[RemoteSite]) -> Vec<Site> {
    let mut sites: Vec<Site> = SITES
        .into_iter()
        .map(|preset| Site {
            name: preset.name.to_owned(),
            site: RemoteSite {
                api: preset.api,
                base: preset.base.to_owned(),
            },
            custom: false,
        })
        .collect();
    let added = added.iter().filter_map(|saved| {
        Some(RemoteSite {
            api: api_of(&saved.api)?,
            base: clean_base(&saved.base).ok()?,
        })
    });
    let others = added
        .map(|site| (site, true))
        .chain(chosen.iter().cloned().map(|site| (site, false)));
    for (site, custom) in others {
        if !sites.iter().any(|known| known.site == site) {
            sites.push(Site {
                name: host_of(&site.base),
                site,
                custom,
            });
        }
    }
    sites
}

/// The chosen sites after a click on `site`'s chip, in the order of the
/// chips. One always stays on, and no more go on than a search takes.
fn toggled(chosen: &[RemoteSite], site: &RemoteSite, order: &[Site]) -> Vec<RemoteSite> {
    let mut next = chosen.to_vec();
    match next.iter().position(|s| s == site) {
        Some(at) if next.len() > 1 => {
            next.remove(at);
        }
        Some(_) => {}
        None if next.len() < MAX_SEARCH_SITES => next.push(site.clone()),
        None => {}
    }
    next.sort_by_key(|s| {
        order
            .iter()
            .position(|known| known.site == *s)
            .unwrap_or(usize::MAX)
    });
    next
}

/// The name of the site at `base`: a preset's, else its address.
pub(super) fn site_name(base: &str) -> String {
    let base = tidy(base);
    SITES
        .into_iter()
        .find(|s| s.base == base)
        .map_or_else(|| host_of(&base), |s| s.name.to_owned())
}

/// Whether two searches go to the same sites.
fn same_sites(a: &[RemoteSite], b: &[RemoteSite]) -> bool {
    let keys = |sites: &[RemoteSite]| {
        let mut keys: Vec<String> = sites.iter().map(site_key).collect();
        keys.sort();
        keys
    };
    keys(a) == keys(b)
}

/// The name a search is saved under: the search itself, or its sites when it
/// is the newest posts.
fn search_name(q: &str, sites: &[RemoteSite]) -> String {
    let name = if q.is_empty() {
        sites
            .iter()
            .map(|s| site_name(&s.base))
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        q.to_owned()
    };
    name.chars().take(MAX_SEARCH_NAME_CHARS).collect()
}

// ---------- results ----------

/// One page from each site as one list: the first post of each in turn, then
/// the second of each, and so on.
fn interleave(pages: Vec<Vec<RemotePost>>) -> Vec<RemotePost> {
    let longest = pages.iter().map(Vec::len).max().unwrap_or(0);
    let mut pages: Vec<_> = pages.into_iter().map(Vec::into_iter).collect();
    let mut out = Vec::new();
    for _ in 0..longest {
        out.extend(pages.iter_mut().filter_map(Iterator::next));
    }
    out
}

/// Adds `more` to `all`, leaving out posts already there. Says how many were
/// new.
fn append(all: &mut Vec<Rc<RemotePost>>, more: Vec<RemotePost>) -> usize {
    let mut seen: HashSet<String> = all.iter().map(|p| p.page_url.clone()).collect();
    let before = all.len();
    for post in more {
        if seen.insert(post.page_url.clone()) {
            all.push(Rc::new(post));
        }
    }
    all.len().saturating_sub(before)
}

/// The same file on another site, left out of the grid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Twin {
    /// The site's address.
    pub site: String,
    pub page_url: String,
}

/// A post in the grid, and where else its file is.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Found {
    pub post: Rc<RemotePost>,
    pub also: Vec<Twin>,
}

/// The results as the grid shows them. With `hide`, a post whose file (by
/// its MD5) was already shown is left out and noted on the one that stays. A
/// post without an MD5 is never a duplicate.
pub(super) fn shown(all: &[Rc<RemotePost>], hide: bool) -> Vec<Found> {
    let mut out: Vec<Found> = Vec::with_capacity(all.len());
    let mut by_md5: HashMap<&str, usize> = HashMap::new();
    for post in all {
        let md5 = post.md5.as_deref().filter(|md5| hide && !md5.is_empty());
        if let Some(first) = md5.and_then(|md5| by_md5.get(md5).copied()) {
            if let Some(kept) = out.get_mut(first) {
                kept.also.push(Twin {
                    site: post.site.clone(),
                    page_url: post.page_url.clone(),
                });
            }
            continue;
        }
        if let Some(md5) = md5 {
            by_md5.insert(md5, out.len());
        }
        out.push(Found {
            post: post.clone(),
            also: Vec::new(),
        });
    }
    out
}

/// How saving a post here is going.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Status {
    /// Waiting or downloading, with how much is done when that is known.
    Busy(Option<u8>),
    /// In the collection: as which post, when that is known, and what the
    /// server said about it.
    Saved(Option<i64>, String),
    Failed(String),
}

/// Where `post` stands: saved before, or what its latest import says.
pub(super) fn status_of(post: &RemotePost, jobs: &HashMap<String, ImportJob>) -> Option<Status> {
    let job = jobs.get(&post.page_url);
    let said = |job: &ImportJob| job.error.clone().unwrap_or_default();
    match job.map(|job| (job, job.state, job.post_id)) {
        Some((job, JobState::Queued | JobState::Running, _)) => {
            Some(Status::Busy(job.progress.and_then(|p| p.percent())))
        }
        Some((_, JobState::Done, Some(id))) => Some(Status::Saved(Some(id), String::new())),
        _ if post.post_id.is_some() => Some(Status::Saved(post.post_id, String::new())),
        // Imported and deleted since, or the file was already here.
        Some((job, JobState::Done | JobState::Duplicate, _)) => {
            Some(Status::Saved(None, said(job)))
        }
        Some((job, JobState::Failed, _)) => Some(Status::Failed(said(job))),
        None => None,
    }
}

/// The post here that `post` has become since it was listed, by its import.
fn newly_saved(post: &RemotePost, jobs: &HashMap<String, ImportJob>) -> Option<i64> {
    jobs.get(&post.page_url)
        .filter(|job| post.post_id.is_none() && job.state == JobState::Done)
        .and_then(|job| job.post_id)
}

/// Notes on each post the import that has finished since, so it stays saved
/// after the job is forgotten. Says whether anything changed.
fn mark_saved(all: &mut [Rc<RemotePost>], jobs: &HashMap<String, ImportJob>) -> bool {
    let mut changed = false;
    for post in all.iter_mut() {
        if let Some(id) = newly_saved(post, jobs) {
            Rc::make_mut(post).post_id = Some(id);
            changed = true;
        }
    }
    changed
}

/// The same for the kept results, from a page that isn't the grid.
pub(super) fn note_saved(jobs: &HashMap<String, ImportJob>) {
    let news = KEPT
        .peek()
        .as_ref()
        .is_some_and(|k| k.all.iter().any(|p| newly_saved(p, jobs).is_some()));
    if news && let Some(kept) = KEPT.write().as_mut() {
        mark_saved(&mut kept.all, jobs);
    }
}

/// The address to show one of a post's pictures from. The server's own
/// (`/api/v1/remote/media…`, for sites that refuse other pages) come without
/// its address.
pub(super) fn picture_src(client: &Client, url: &str) -> String {
    if url.starts_with('/') {
        client.absolute(url)
    } else {
        url.to_owned()
    }
}

/// A picture's size for the layout. A site that doesn't say gives 0.
fn size_of(post: &RemotePost) -> (Option<i32>, Option<i32>) {
    let side = |n: u32| i32::try_from(n).ok().filter(|n| *n > 0);
    (side(post.width), side(post.height))
}

/// How far one site has been read.
#[derive(Clone, Debug, PartialEq, Eq)]
struct SitePage {
    site: RemoteSite,
    /// The page to ask for next.
    next: u32,
    more: bool,
    /// What went wrong the last time it was asked.
    error: Option<String>,
}

impl SitePage {
    fn first(site: &RemoteSite) -> Self {
        Self {
            site: site.clone(),
            next: 1,
            more: true,
            error: None,
        }
    }

    /// Whether the next page is worth asking for.
    fn wanted(&self) -> bool {
        self.more && self.error.is_none()
    }
}

/// The last search with what it found, kept while the app runs.
#[derive(Clone)]
struct Kept {
    owner: String,
    q: String,
    sites: Vec<RemoteSite>,
    all: Vec<Rc<RemotePost>>,
    pages: Vec<SitePage>,
    loaded_at: DateTime<Utc>,
    /// How far down the page was.
    y: f64,
}

impl Kept {
    fn is(&self, q: &str, sites: &[RemoteSite]) -> bool {
        self.owner == store::owner() && self.q == q && self.sites == sites
    }
}

/// The last search's results, so that coming back from a post shows the grid
/// where it was and asks no site again.
static KEPT: GlobalSignal<Option<Kept>> = Signal::global(|| None);
/// The saved searches last seen and whose they are, so their chips don't push
/// the grid down when they load.
static SAVED: GlobalSignal<Option<(String, Vec<SavedRemoteSearch>)>> = Signal::global(|| None);

/// What is kept for this search on these sites, while it is fresh enough for
/// its picture addresses to still work.
fn kept_for(q: &str, sites: &[RemoteSite]) -> Option<Kept> {
    let now = Utc::now();
    KEPT.peek()
        .as_ref()
        .filter(|k| k.is(q, sites) && (now - k.loaded_at).num_hours() < MAX_KEPT_HOURS)
        .cloned()
}

/// How far down the page was for this search, when it is the one kept.
fn kept_y(q: &str, sites: &[RemoteSite]) -> f64 {
    KEPT.peek()
        .as_ref()
        .filter(|k| k.is(q, sites))
        .map_or(0.0, |k| k.y)
}

/// A search as the sites get it: single spaces, none at the ends.
fn tidy_query(q: &str) -> String {
    q.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Where Explore's boorus side opens: back on the last search, as the
/// Gallery tab is on its own.
pub(super) fn last_route() -> Route {
    let kept = KEPT.peek();
    let kept = kept.as_ref().filter(|k| k.owner == store::owner());
    search_route(
        &kept.map(|k| k.q.clone()).unwrap_or_default(),
        &kept.map(|k| sites_param(&k.sites)).unwrap_or_default(),
    )
}

/// A kept post with its neighbours in the grid, for the post's own page.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Around {
    pub found: Found,
    pub prev: Option<String>,
    pub next: Option<String>,
    /// The kind of site it is on.
    pub api: Option<BooruApi>,
}

/// The kept post whose page is `u`. A duplicate left out of the grid has no
/// neighbours.
pub(super) fn around(u: &str) -> Option<Around> {
    let kept = KEPT.peek();
    let kept = kept.as_ref().filter(|k| k.owner == store::owner())?;
    let hide = PREFS.peek().booru_dedupe && kept.sites.len() > 1;
    let list = shown(&kept.all, hide);
    let api_of_base = |base: &str| {
        kept.sites
            .iter()
            .find(|s| tidy(&s.base) == tidy(base))
            .map(|s| s.api)
    };
    if let Some(at) = list.iter().position(|f| f.post.page_url == u) {
        let page = |i: Option<usize>| i.and_then(|i| list.get(i)).map(|f| f.post.page_url.clone());
        let found = list.get(at)?.clone();
        return Some(Around {
            api: api_of_base(&found.post.site),
            prev: page(at.checked_sub(1)),
            next: page(at.checked_add(1)),
            found,
        });
    }
    let post = kept.all.iter().find(|p| p.page_url == u)?.clone();
    Some(Around {
        api: api_of_base(&post.site),
        found: Found {
            post,
            also: Vec::new(),
        },
        prev: None,
        next: None,
    })
}

/// Queues the import of one post, from its own page.
pub(super) async fn save_one(post: &RemotePost) -> Result<(), String> {
    let client = store::client().ok_or_else(|| t!("activity-signed-out"))?;
    let request = import_request(post, "").map_err(|e| e.to_string())?;
    activity::queue_imports(&client, vec![request]).await
}

/// What the scroll watcher says (`WATCH_SCROLL`); only how far down matters
/// here.
#[derive(Deserialize)]
struct Scrolled {
    y: f64,
}

/// Puts the page back where it was, once the grid is tall enough to be there.
const JUMP_TO_Y: &str = r#"
const y = await dioxus.recv();
const go = (tries) => {
  const tall = document.documentElement.scrollHeight - innerHeight;
  if (tall < y && tries > 0) return requestAnimationFrame(() => go(tries - 1));
  scrollTo({ top: y, behavior: "instant" });
  window.__sbNear?.check(true);
};
go(5);
"#;
/// On a phone the chips are one row to scroll: the first site searched is
/// brought into it.
const SHOW_CHOSEN: &str = r#"
const row = document.querySelector(".booru-sites");
const chip = row?.querySelector(".chip.on");
if (chip && row.scrollWidth > row.clientWidth) {
  const [box, all] = [chip.getBoundingClientRect(), row.getBoundingClientRect()];
  if (box.left < all.left || box.right > all.right - 44) row.scrollLeft += box.left - all.left - 8;
}
"#;
/// Goes to the top at once, for a new search.
const TO_TOP_NOW: &str = r#"scrollTo({ top: 0, behavior: "instant" });"#;
/// Glides to the top, unless motion is to be spared.
const TO_TOP: &str = r#"
const calm = matchMedia("(prefers-reduced-motion: reduce)").matches
  || !!document.querySelector('.app[data-saver="on"]');
scrollTo({ top: 0, behavior: calm ? "instant" : "smooth" });
"#;

/// The booru browser: one search (`q`) on the chosen sites (`s`), in the
/// gallery's grid.
#[component]
pub fn Boorus(q: String, s: String) -> Element {
    let query = tidy_query(&from_address(&q));
    let sites = chosen_sites(&s, &PREFS.read().booru_sites);
    let key = (query.clone(), sites.clone());

    // What was kept is there from the first frame; `restart` takes over after.
    let kept = || kept_for(&query, &sites);
    let mut all = use_signal(|| kept().map(|k| k.all).unwrap_or_default());
    let mut pages = use_signal(|| kept().map(|k| k.pages).unwrap_or_default());
    let mut current = use_signal(|| key.clone());
    let mut loading = use_signal(|| all.peek().is_empty());
    let mut loaded_at = use_signal(Utc::now);
    let mut error = use_signal(|| None::<String>);
    // Bumped on every new search, so a late answer to an old one is dropped.
    let mut generation = use_signal(|| 0u64);
    // Bumped to put the page back where it was.
    let mut jump = use_signal(|| 0u64);
    let mut near = use_signal(|| {
        let y = kept().map_or(0.0, |k| k.y);
        (y - FIRST_NEAR.0, y + FIRST_NEAR.1)
    });

    let list = use_memo(move || {
        let hide = PREFS.read().booru_dedupe && current.read().1.len() > 1;
        shown(&all.read(), hide)
    });
    // Tiles below this index aren't new, and don't animate in.
    let mut settled = use_signal(|| list.peek().len());
    use_effect(move || settled.set(list.read().len()));
    let grid = use_memo(|| Grid::new(&PREFS.read(), GRID_WIDTH()));
    let tiles = use_memo(move || grid().lay(list.read().iter().map(|f| size_of(&f.post))));

    let keep = move || {
        let (q, sites) = current.peek().clone();
        let y = kept_y(&q, &sites);
        *KEPT.write() = Some(Kept {
            owner: store::owner(),
            q,
            sites,
            all: all.peek().clone(),
            pages: pages.peek().clone(),
            loaded_at: *loaded_at.peek(),
            y,
        });
    };

    // One more page from every site that has one.
    let mut load = move || {
        if *loading.peek() {
            return;
        }
        let Some(client) = store::client() else {
            return;
        };
        let (q, _) = current.peek().clone();
        let asked: Vec<(usize, RemoteSearch)> = pages
            .peek()
            .iter()
            .enumerate()
            .filter(|(_, page)| page.wanted())
            .map(|(index, page)| {
                let search = RemoteSearch {
                    api: page.site.api,
                    base: page.site.base.clone(),
                    tags: q.clone(),
                    page: page.next,
                    // The server uses the account saved for the site.
                    login: None,
                    api_key: None,
                };
                (index, search)
            })
            .collect();
        if asked.is_empty() {
            return;
        }
        let at = *generation.peek();
        loading.set(true);
        spawn(async move {
            let answers =
                join_all(asked.iter().map(|(_, search)| client.remote_search(search))).await;
            if *generation.peek() != at {
                return;
            }
            if answers
                .iter()
                .any(|a| a.as_ref().is_err_and(|e| e.is_unauthorized()))
            {
                store::forget_session();
                return;
            }
            let known: HashSet<String> = all.peek().iter().map(|p| p.page_url.clone()).collect();
            let mut found = Vec::new();
            {
                // One site's trouble is its own: the others' posts still show.
                let mut pages = pages.write();
                for ((index, _), answer) in asked.iter().zip(answers) {
                    let Some(page) = pages.get_mut(*index) else {
                        continue;
                    };
                    match answer {
                        Ok(result) => {
                            // A site that only repeats itself has no more to give.
                            let fresh = result.posts.iter().any(|p| !known.contains(&p.page_url));
                            page.more = result.more && fresh;
                            page.next = page.next.saturating_add(1);
                            found.push(result.posts);
                        }
                        Err(e) => {
                            page.error = Some(e.to_string());
                            found.push(Vec::new());
                        }
                    }
                }
            }
            append(&mut all.write(), interleave(found));
            let failed = pages.peek().iter().find_map(|p| p.error.clone());
            error.set(failed.filter(|_| all.peek().is_empty()));
            loading.set(false);
            keep();
        });
    };

    let mut selecting = use_signal(|| false);
    let mut selected = use_signal(HashSet::<String>::new);
    // The last tile clicked, for shift-click ranges.
    let mut anchor = use_signal(|| None::<usize>);
    let mut stop_selecting = move || {
        selecting.set(false);
        selected.write().clear();
        anchor.set(None);
    };

    let mut restart = move |q: String, sites: Vec<RemoteSite>| {
        generation += 1;
        let keys: Vec<String> = sites.iter().map(site_key).collect();
        if PREFS.peek().booru_sites != keys {
            PREFS.write().booru_sites = keys;
        }
        current.set((q.clone(), sites.clone()));
        error.set(None);
        stop_selecting();
        loading.set(false);
        if let Some(kept) = kept_for(&q, &sites) {
            near.set((kept.y - FIRST_NEAR.0, kept.y + FIRST_NEAR.1));
            loaded_at.set(kept.loaded_at);
            error.set(
                kept.pages
                    .iter()
                    .find_map(|p| p.error.clone())
                    .filter(|_| kept.all.is_empty()),
            );
            // Nothing that was there before makes an entrance.
            settled.set(kept.all.len());
            all.set(kept.all);
            pages.set(kept.pages);
            jump += 1;
            return;
        }
        all.write().clear();
        settled.set(0);
        pages.set(sites.iter().map(SitePage::first).collect());
        loaded_at.set(Utc::now());
        near.set((-FIRST_NEAR.0, FIRST_NEAR.1));
        document::eval(TO_TOP_NOW);
        load();
    };
    // Bumped to ask the sites again, in this scope.
    let mut reload = use_signal(|| 0u64);
    use_effect(use_reactive!(|key| {
        reload();
        restart(key.0, key.1);
    }));
    let mut again = move || {
        *KEPT.write() = None;
        reload += 1;
    };

    // More comes before the user gets to the end of what is there.
    let has_more = use_memo(move || pages.read().iter().any(SitePage::wanted));
    use_effect(move || {
        let (_, to) = near();
        let Grid { gap, tile, .. } = grid();
        let end = tiles.read().height.px(tile, gap);
        if tile > 0.0 && to >= end && generation() > 0 && has_more() && !loading() {
            load();
        }
    });

    // Remembers how far down the page is, for coming back to it.
    use_hook(move || {
        spawn(async move {
            let mut watch = document::eval(WATCH_SCROLL);
            while let Ok(seen) = watch.recv::<Scrolled>().await {
                let (q, sites) = current.peek().clone();
                if let Some(kept) = KEPT.write().as_mut().filter(|k| k.is(&q, &sites)) {
                    kept.y = seen.y;
                }
            }
        });
    });
    use_drop(|| {
        document::eval(WATCH_SCROLL_OFF);
        document::eval(WATCH_NEAR_OFF);
    });
    use_effect(move || {
        if jump() == 0 {
            return;
        }
        let (q, sites) = current.peek().clone();
        let y = kept_y(&q, &sites);
        let eval = document::eval(JUMP_TO_Y);
        let _ = eval.send(y);
    });

    // A post saved from here stays marked after its job is forgotten.
    use_effect(move || {
        let jobs = activity::jobs_by_source(&JOBS.read());
        let news = all.peek().iter().any(|p| newly_saved(p, &jobs).is_some());
        if news && mark_saved(&mut all.write(), &jobs) {
            keep();
        }
    });

    // The accounts saved on the server. What an older version kept on this
    // device moves there first, sealed.
    let mut accounts = use_signal(Vec::<BooruAccount>::new);
    let mut saved = use_signal(|| {
        let owner = store::owner();
        SAVED
            .peek()
            .as_ref()
            .filter(|(whose, _)| *whose == owner)
            .map(|(_, list)| list.clone())
    });
    use_hook(move || {
        let Some(client) = store::client() else {
            return;
        };
        spawn(async move {
            if let Ok(list) = client.remote_searches().await {
                *SAVED.write() = Some((store::owner(), list.clone()));
                if saved.peek().as_ref() != Some(&list) {
                    saved.set(Some(list));
                }
            }
            let save = |account: SaveBooruAccount| {
                let client = client.clone();
                async move { client.save_booru_account(&account).await.is_ok() }
            };
            move_accounts(PREFS.signal(), save).await;
            if let Ok(list) = client.booru_accounts().await {
                accounts.set(list);
            }
        });
    });

    let chips = all_sites(&PREFS.read().boorus, &sites);
    use_effect(use_reactive!(|s| {
        // Again whenever the address names other sites.
        let _ = s;
        document::eval(SHOW_CHOSEN);
    }));
    let go = {
        let query = query.clone();
        move |next: Vec<RemoteSite>| {
            navigator().replace(search_route(&query, &sites_param(&next)));
        }
    };
    let mut sheet = use_signal(|| false);
    let onadd = {
        let (sites, chips, go) = (sites.clone(), chips.clone(), go.clone());
        move |site: RemoteSite| {
            if preset_of(&site).is_none() {
                let added = SavedBooru {
                    api: site.api.as_str().to_owned(),
                    base: site.base.clone(),
                    ..SavedBooru::default()
                };
                remember(&mut PREFS.write().boorus, added);
            }
            if !sites.contains(&site) && sites.len() < MAX_SEARCH_SITES {
                let mut order = chips.clone();
                order.push(Site {
                    name: host_of(&site.base),
                    site: site.clone(),
                    custom: true,
                });
                go(toggled(&sites, &site, &order));
            }
        }
    };
    let onremove = {
        let (sites, go) = (sites.clone(), go.clone());
        move |site: RemoteSite| {
            PREFS
                .write()
                .boorus
                .retain(|saved| tidy(&saved.base) != tidy(&site.base));
            if sites.contains(&site) {
                let rest: Vec<RemoteSite> = sites.iter().filter(|s| **s != site).cloned().collect();
                // With none left the address names none, and the first preset shows.
                if rest.is_empty() {
                    PREFS.write().booru_sites.clear();
                }
                go(rest);
            }
        }
    };

    // One handler kept between renders, or every tile would render again
    // whenever the part of the grid near the screen moved.
    let toggle = use_callback(move |(index, shift): (usize, bool)| {
        let list = list.read();
        let jobs = activity::jobs_by_source(&JOBS.peek());
        let free = |found: &Found| status_of(&found.post, &jobs).is_none();
        let Some(clicked) = list.get(index).filter(|found| free(found)) else {
            return;
        };
        let page = clicked.post.page_url.clone();
        let mut set = selected.write();
        let range = anchor()
            .filter(|_| shift)
            .map(|a| a.min(index)..=a.max(index));
        if let Some(range) = range {
            let on = !set.contains(&page);
            for found in list.iter().skip(*range.start()).take(range.count()) {
                if on && free(found) {
                    set.insert(found.post.page_url.clone());
                } else if !on {
                    set.remove(&found.post.page_url);
                }
            }
        } else if !set.remove(&page) {
            set.insert(page);
        }
        anchor.set(Some(index));
    });
    // Notes where the page is before leaving it: the watcher waits for the
    // scrolling to stop, and a click can come sooner.
    let open = use_callback(move |post: Route| {
        spawn(async move {
            if let Ok(y) = document::eval("return scrollY").join::<f64>().await {
                let (q, sites) = current.peek().clone();
                if let Some(kept) = KEPT.write().as_mut().filter(|k| k.is(&q, &sites)) {
                    kept.y = y;
                }
            }
            navigator().push(post);
        });
    });

    let mut saving = use_signal(|| false);
    let mut save_error = use_signal(|| None::<String>);
    let save = use_callback(move |extra: String| {
        if saving() {
            return;
        }
        let Some(client) = store::client() else {
            return;
        };
        let requests: Result<Vec<ImportRequest>, TagError> = list
            .read()
            .iter()
            .filter(|found| selected.read().contains(&found.post.page_url))
            .map(|found| import_request(&found.post, &extra))
            .collect();
        let requests = match requests {
            Ok(requests) if requests.is_empty() => return,
            Ok(requests) => requests,
            Err(e) => {
                save_error.set(Some(e.to_string()));
                return;
            }
        };
        saving.set(true);
        save_error.set(None);
        // The server runs the imports; their progress comes from `JOBS`.
        spawn(async move {
            match activity::queue_imports(&client, requests).await {
                Ok(()) => stop_selecting(),
                Err(message) => save_error.set(Some(message)),
            }
            saving.set(false);
        });
    });

    let Grid {
        columns,
        gap,
        width,
        tile,
        ..
    } = grid();
    // A new layout moves everything: back to the same place down the page.
    let mut laid_out = use_signal(|| *grid.peek());
    use_effect(move || {
        let now = grid();
        if *laid_out.peek() != now {
            laid_out.set(now);
            jump += 1;
        }
    });
    let dedupe = PREFS.read().booru_dedupe;
    let many = sites.len() > 1;
    let jobs = activity::jobs_by_source(&JOBS.read());
    let placed = tiles.read();
    let found = list.read();
    let (from, to) = near();
    let on_page: Vec<_> = placed
        .within(tile, gap, from, to)
        .into_iter()
        .filter_map(|index| Some((index, found.get(index)?, *placed.tiles.get(index)?)))
        .collect();
    let (seen_from, seen_to) = may_be_on_screen((from, to));
    let in_sight = move |place: &Place| {
        let top = place.top.px(tile, gap);
        top < seen_to && top + place.height.px(tile, gap) > seen_from
    };
    let failed: Vec<(RemoteSite, String)> = pages
        .read()
        .iter()
        .filter_map(|p| Some((p.site.clone(), p.error.clone()?)))
        .collect();
    let is_empty = found.is_empty() && !loading() && error().is_none() && !has_more();
    // What can still be picked: not what is saved or on its way.
    let free: Vec<String> = found
        .iter()
        .filter(|f| selecting() && status_of(&f.post, &jobs).is_none())
        .map(|f| f.post.page_url.clone())
        .collect();

    rsx! {
        div { class: "page boorus",
            div { class: "page-head",
                // The search box above shows the search; the two sides of
                // Explore stand where the gallery has its title.
                h1 { class: "page-title visually-hidden",
                    if query.is_empty() { {t!("nav-boorus")} } else { "{query}" }
                }
                Sides { boorus: true }
                div { class: "head-tools",
                    SaveSearch { q: query.clone(), sites: sites.clone(), saved }
                    if many {
                        button {
                            class: if dedupe { "icon-btn dedupe on" } else { "icon-btn dedupe" },
                            r#type: "button",
                            title: t!("boorus-dedupe"),
                            aria_label: t!("boorus-dedupe"),
                            aria_pressed: dedupe,
                            onclick: move |_| {
                                PREFS.write().booru_dedupe = !dedupe;
                            },
                            Ico { icon: Icon::Copy }
                        }
                    }
                    ViewMenu { width, gap }
                    button {
                        class: if selecting() { "icon-btn select-toggle on" } else { "icon-btn select-toggle" },
                        r#type: "button",
                        title: if selecting() { t!("gallery-stop-selecting") } else { t!("gallery-select") },
                        aria_label: if selecting() { t!("gallery-stop-selecting") } else { t!("gallery-select") },
                        aria_pressed: selecting(),
                        onclick: move |_| {
                            if selecting() {
                                stop_selecting();
                            } else {
                                save_error.set(None);
                                selecting.set(true);
                            }
                        },
                        Ico { icon: if selecting() { Icon::Close } else { Icon::Check } }
                    }
                }
            }
            div { class: "booru-sites", role: "group", aria_label: t!("boorus-sites"),
                for chip in chips.iter() {
                    SiteChip {
                        key: "{site_key(&chip.site)}",
                        name: chip.name.clone(),
                        on: sites.contains(&chip.site),
                        error: failed.iter().find(|(site, _)| *site == chip.site).map(|(_, e)| e.clone()),
                        onclick: {
                            let (sites, chips, site, go) = (sites.clone(), chips.clone(), chip.site.clone(), go.clone());
                            move |()| go(toggled(&sites, &site, &chips))
                        },
                    }
                }
                if !failed.is_empty() && !loading() {
                    button {
                        class: "icon-btn small",
                        r#type: "button",
                        title: t!("retry"),
                        aria_label: t!("retry"),
                        onclick: move |_| again(),
                        Ico { icon: Icon::Restore, size: 20 }
                    }
                }
                button {
                    class: "icon-btn small sites-edit",
                    r#type: "button",
                    title: t!("boorus-sites"),
                    aria_label: t!("boorus-sites"),
                    aria_haspopup: "dialog",
                    onclick: move |_| sheet.set(true),
                    Ico { icon: Icon::Tune, size: 20 }
                }
            }
            if query.is_empty() {
                SavedList { saved }
            }
            if let Some(message) = error() {
                ErrorBanner { message, onretry: move |()| again() }
            }
            if is_empty {
                div { class: "card empty",
                    Ico { icon: Icon::Search, size: 48 }
                    h2 { {t!("no-matches")} }
                }
            }
            div {
                class: if selecting() { "masonry-box selecting" } else { "masonry-box" },
                onmounted: move |_| {
                    spawn(async move {
                        let mut watch = document::eval(WATCH_WIDTH);
                        while let Ok(seen) = watch.recv::<f64>().await {
                            if *GRID_WIDTH.peek() != seen {
                                *GRID_WIDTH.write() = seen;
                            }
                        }
                    });
                    spawn(async move {
                        let mut watch = document::eval(WATCH_NEAR);
                        while let Ok(seen) = watch.recv::<(f64, f64)>().await {
                            near.set(seen);
                        }
                    });
                },
                div {
                    class: "masonry",
                    style: "--columns: {columns}; --height: {placed.height.tiles:.5}; --height-gaps: {placed.height.gaps:.5}",
                    for (index, found, place) in on_page {
                        RemoteThumb {
                            key: "{found.post.page_url}",
                            post: found.post.clone(),
                            index,
                            place,
                            site: many.then(|| site_name(&found.post.site)),
                            status: status_of(&found.post, &jobs),
                            selecting: selecting(),
                            on: selected.read().contains(&found.post.page_url),
                            still: index < settled() || !in_sight(&place),
                            to: Route::BooruPost {
                                u: to_address(&found.post.page_url),
                                q: to_address(&query),
                                s: s.clone(),
                            },
                            onopen: open,
                            ontoggle: toggle,
                        }
                    }
                }
            }
            if has_more() {
                div { class: "sentinel",
                    if loading() {
                        Spinner {}
                    } else {
                        button { class: "btn tonal", onclick: move |_| load(), {t!("load-more")} }
                    }
                }
            } else if loading() {
                div { class: "sentinel", Spinner {} }
            }
            if selecting() {
                SaveBar { selected, free, busy: saving(), error: save_error(), onsave: save }
            }
        }
        ToTop { selecting: selecting() }
        if sheet() {
            SitesSheet {
                sites: chips.clone(),
                accounts,
                onadd,
                onremove,
                onclose: move |()| sheet.set(false),
            }
        }
    }
}

/// A site in the row under the title: a click searches it too, or no longer.
#[component]
fn SiteChip(name: String, on: bool, error: Option<String>, onclick: EventHandler<()>) -> Element {
    let class = match (on, error.is_some()) {
        (_, true) => "chip on warn",
        (true, false) => "chip on",
        (false, false) => "chip",
    };
    rsx! {
        button {
            class,
            r#type: "button",
            aria_pressed: on,
            title: error.unwrap_or_default(),
            onclick: move |_| onclick.call(()),
            "{name}"
        }
    }
}

/// Star in the header: keeps this search and its sites, or forgets it.
#[component]
fn SaveSearch(
    q: String,
    sites: Vec<RemoteSite>,
    saved: Signal<Option<Vec<SavedRemoteSearch>>>,
) -> Element {
    let mut saved = saved;
    let mut busy = use_signal(|| false);
    let existing = saved.read().as_ref().and_then(|list| {
        list.iter()
            .find(|s| s.query == q && same_sites(&s.sites, &sites))
            .map(|s| s.id)
    });
    let label = if existing.is_some() {
        t!("search-forget")
    } else {
        t!("search-save")
    };
    let toggle = move |_| {
        let Some(client) = store::client() else {
            return;
        };
        let search = SaveRemoteSearch {
            name: search_name(&q, &sites),
            query: q.clone(),
            sites: sites.clone(),
        };
        busy.set(true);
        spawn(async move {
            let result = match existing {
                Some(id) => client.delete_remote_search(id).await.map(|()| None),
                None => client.save_remote_search(&search).await.map(Some),
            };
            if let Ok(added) = result
                && let Some(list) = saved.write().as_mut()
            {
                list.retain(|s| Some(s.id) != existing);
                if let Some(s) = added {
                    list.push(s);
                    list.sort_by_key(|s| s.name.to_lowercase());
                }
                *SAVED.write() = Some((store::owner(), list.clone()));
            }
            busy.set(false);
        });
    };
    rsx! {
        button {
            class: if existing.is_some() { "icon-btn save-search on" } else { "icon-btn save-search" },
            r#type: "button",
            title: label.clone(),
            aria_label: label,
            aria_pressed: existing.is_some(),
            disabled: busy() || saved.read().is_none(),
            onclick: toggle,
            Ico { icon: if existing.is_some() { Icon::Star } else { Icon::StarOutline } }
        }
    }
}

/// The saved searches as chips, while nothing is searched. Each goes to its
/// own sites.
#[component]
fn SavedList(saved: Signal<Option<Vec<SavedRemoteSearch>>>) -> Element {
    let list = saved.read();
    let Some(list) = list.as_ref().filter(|l| !l.is_empty()) else {
        return rsx! {};
    };
    rsx! {
        nav { class: "saved-searches", aria_label: t!("saved-searches"),
            for s in list.iter() {
                Link {
                    key: "{s.id}",
                    class: "chip",
                    title: search_name(&s.query, &s.sites),
                    to: search_route(&s.query, &sites_param(&s.sites)),
                    Ico { icon: Icon::Star, size: 16 }
                    "{s.name}"
                }
            }
        }
    }
}

/// A result in the grid: the gallery's tile, with the site it is on and how
/// saving it here stands. A click opens its page; while selecting, it picks
/// it. `still` skips the entrance, and only its value when the tile appears
/// counts.
#[component]
fn RemoteThumb(
    post: Rc<RemotePost>,
    index: usize,
    place: Place,
    /// The site's name, when more than one is searched.
    site: Option<String>,
    status: Option<Status>,
    selecting: bool,
    on: bool,
    still: bool,
    /// The post's own page here.
    to: Route,
    onopen: EventHandler<Route>,
    ontoggle: EventHandler<(usize, bool)>,
) -> Element {
    let still = use_hook(|| still);
    let src = store::client()
        .filter(|_| !post.preview_url.is_empty())
        .map(|client| picture_src(&client, &post.preview_url));
    let delay = (index % 40) * 16;
    let style = format!(
        "--delay: {delay}ms; --x: {:.5}; --x-gaps: {:.5}; --y: {:.5}; --y-gaps: {:.5}\
         ; --w: {:.5}; --w-gaps: {:.5}; --h: {:.5}; --h-gaps: {:.5}",
        place.left.tiles,
        place.left.gaps,
        place.top.tiles,
        place.top.gaps,
        place.width.tiles,
        place.width.gaps,
        place.height.tiles,
        place.height.gaps,
    );
    let alt = post
        .tags
        .iter()
        .take(8)
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    let label = if alt.is_empty() {
        t!("post-alt", id = post.id.to_string())
    } else {
        alt.clone()
    };
    // What is saved or on its way can't be picked again.
    let pickable = selecting && status.is_none();
    let class = match (pickable, on, still) {
        (true, true, false) => "thumb selectable on",
        (true, true, true) => "thumb selectable on still",
        (true, false, false) => "thumb selectable",
        (true, false, true) => "thumb selectable still",
        (false, _, false) => "thumb",
        (false, _, true) => "thumb still",
    };
    let href = to.to_string();
    let video = post.is_video();
    let mark = match status {
        None => rsx! {},
        Some(Status::Busy(percent)) => rsx! {
            span { class: "pill busy remote-status", role: "status", title: t!("upload-job-running"),
                if let Some(percent) = percent { "{percent}%" } else { "…" }
            }
        },
        Some(Status::Saved(Some(id), _)) => rsx! {
            span {
                class: "remote-mark",
                role: "link",
                title: t!("post-number", id = id.to_string()),
                // Straight to the post here, past the tile's own click.
                onclick: move |e: MouseEvent| {
                    e.prevent_default();
                    e.stop_propagation();
                    navigator().push(Route::PostView { id, q: String::new() });
                },
                Ico { icon: Icon::Check, size: 18 }
            }
        },
        Some(Status::Saved(None, note)) => rsx! {
            span {
                class: "remote-mark",
                title: if note.is_empty() { t!("boorus-known") } else { note },
                Ico { icon: Icon::Check, size: 18 }
            }
        },
        Some(Status::Failed(message)) => rsx! {
            span { class: "pill bad remote-status", role: "status", title: "{message}", {t!("upload-failed")} }
        },
    };
    rsx! {
        a {
            class,
            style,
            href,
            aria_label: label,
            role: pickable.then_some("button"),
            aria_pressed: pickable.then(|| on.to_string()),
            onclick: move |e: MouseEvent| {
                if selecting {
                    e.prevent_default();
                    ontoggle.call((index, e.modifiers().shift()));
                } else if e.modifiers().is_empty() {
                    // With a modifier the browser opens the link elsewhere.
                    e.prevent_default();
                    onopen.call(to.clone());
                }
            },
            onkeydown: move |e: KeyboardEvent| {
                if selecting && e.key() == Key::Character(" ".to_owned()) {
                    e.prevent_default();
                    ontoggle.call((index, e.modifiers().shift()));
                }
            },
            div { class: "thumb-media",
                if let Some(src) = src {
                    // Only tiles near the screen are on the page.
                    img { src, alt, loading: "eager", decoding: "async", "referrerpolicy": "no-referrer" }
                } else {
                    div { class: "thumb-placeholder", Ico { icon: Icon::Gallery, size: 40 } }
                }
            }
            if let Some(site) = site {
                span { class: "badge remote-site", "{site}" }
            }
            if video {
                span { class: "badge", Ico { icon: Icon::Play, size: 16 } }
            }
            {mark}
            if pickable {
                span { class: "check-mark", aria_hidden: "true", Ico { icon: Icon::Check, size: 20 } }
            }
        }
    }
}

/// The bar of a selection: tags to add to every picked post, and the button
/// that saves them here.
#[component]
fn SaveBar(
    selected: Signal<HashSet<String>>,
    /// The posts on the page that can be picked.
    free: Vec<String>,
    busy: bool,
    error: Option<String>,
    onsave: EventHandler<String>,
) -> Element {
    let mut selected = selected;
    let mut extra = use_signal(String::new);
    let count = selected.read().len();
    let all_selected = !free.is_empty() && free.iter().all(|page| selected.read().contains(page));
    rsx! {
        form {
            class: "bulk-bar glass",
            aria_label: t!("boorus-save-label"),
            onsubmit: move |e: FormEvent| {
                e.prevent_default();
                onsave.call(extra());
            },
            div { class: "bulk-count",
                strong { role: "status", if count == 0 { {t!("gallery-select")} } else { {t!("bulk-selected", count = count)} } }
                button {
                    r#type: "button",
                    class: "btn text small",
                    disabled: free.is_empty(),
                    onclick: move |_| {
                        let mut set = selected.write();
                        if all_selected {
                            set.clear();
                        } else {
                            set.extend(free.iter().cloned());
                        }
                    },
                    if all_selected { {t!("bulk-select-none")} } else { {t!("bulk-select-all")} }
                }
            }
            input {
                class: "bulk-tags",
                r#type: "text",
                placeholder: t!("boorus-extra-tags"),
                aria_label: t!("boorus-extra-tags"),
                autocapitalize: "none",
                spellcheck: false,
                value: "{extra}",
                oninput: move |e| extra.set(e.value()),
            }
            button { class: "btn", r#type: "submit", disabled: busy || count == 0,
                Ico { icon: Icon::Download }
                {t!("boorus-save", count = count)}
            }
            if let Some(message) = error {
                p { class: "bulk-error", role: "alert", "{message}" }
            }
        }
    }
}

/// The arrow back to the top, once the page is a screen down.
#[component]
fn ToTop(selecting: bool) -> Element {
    let mut far = use_signal(|| false);
    use_hook(move || {
        spawn(async move {
            let mut watch = document::eval(WATCH_FAR);
            while let Ok(seen) = watch.recv::<bool>().await {
                if *far.peek() != seen {
                    far.set(seen);
                }
            }
        });
    });
    use_drop(|| {
        document::eval(WATCH_FAR_OFF);
    });
    let shown = far() && !selecting;
    rsx! {
        button {
            class: if shown { "to-top shown" } else { "to-top" },
            r#type: "button",
            title: t!("gallery-to-top"),
            aria_label: t!("gallery-to-top"),
            onclick: move |_| {
                document::eval(TO_TOP);
            },
            Ico { icon: Icon::ArrowUp }
        }
    }
}

/// The sites, over the page: an account for each, saved on the server, and
/// the ones added by their address.
#[component]
fn SitesSheet(
    sites: Vec<Site>,
    accounts: Signal<Vec<BooruAccount>>,
    onadd: EventHandler<RemoteSite>,
    onremove: EventHandler<RemoteSite>,
    onclose: EventHandler<()>,
) -> Element {
    let mut accounts = accounts;
    let mut error = use_signal(|| None::<String>);
    // The address of the site whose account is being typed.
    let mut editing = use_signal(|| None::<String>);
    let mut login = use_signal(String::new);
    let mut key = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut address = use_signal(String::new);
    let mut kind = use_signal(|| BooruApi::Danbooru);

    let save = use_callback(move |site: RemoteSite| {
        let Some(client) = store::client() else {
            return;
        };
        let typed = SaveBooruAccount {
            api: site.api,
            base: site.base,
            login: login(),
            api_key: key(),
        };
        let account = match typed.clean() {
            Ok(account) => account,
            Err(e) => {
                error.set(Some(e.to_string()));
                return;
            }
        };
        busy.set(true);
        error.set(None);
        spawn(async move {
            match client.save_booru_account(&account).await {
                Ok(kept) => {
                    {
                        let mut list = accounts.write();
                        list.retain(|a| a.id != kept.id && tidy(&a.base) != tidy(&kept.base));
                        list.push(kept);
                    }
                    // The key is the server's from here on.
                    editing.set(None);
                    login.set(String::new());
                    key.set(String::new());
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    });
    let forget = use_callback(move |id: i64| {
        let Some(client) = store::client() else {
            return;
        };
        spawn(async move {
            match client.delete_booru_account(id).await {
                Ok(()) => accounts.write().retain(|a| a.id != id),
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    });
    let add = move |e: FormEvent| {
        e.prevent_default();
        match clean_base(&address()) {
            Ok(base) => {
                error.set(None);
                address.set(String::new());
                onadd.call(RemoteSite { api: kind(), base });
            }
            Err(e) => error.set(Some(e.to_string())),
        }
    };

    rsx! {
        div {
            class: "sheet-scrim",
            onclick: move |_| onclose.call(()),
            onkeydown: move |e| {
                if e.key() == Key::Escape {
                    onclose.call(());
                }
            },
            div {
                class: "card sheet sites-sheet",
                role: "dialog",
                aria_modal: "true",
                aria_labelledby: "sites-sheet-title",
                onclick: move |e| e.stop_propagation(),
                div { class: "sheet-head",
                    h2 { id: "sites-sheet-title", class: "card-title", {t!("boorus-sites")} }
                    button { class: "icon-btn", r#type: "button", aria_label: t!("share-close"), title: t!("share-close"),
                        onclick: move |_| onclose.call(()),
                        Ico { icon: Icon::Close }
                    }
                }
                if let Some(message) = error() {
                    ErrorBanner { message }
                }
                ul { class: "site-list",
                    for site in sites {
                        SiteRow {
                            key: "{site_key(&site.site)}",
                            account: accounts.read().iter().find(|a| tidy(&a.base) == tidy(&site.site.base)).cloned(),
                            open: editing.read().as_deref() == Some(site.site.base.as_str()),
                            site,
                            login,
                            secret: key,
                            busy: busy(),
                            onedit: move |base: Option<String>| {
                                login.set(String::new());
                                key.set(String::new());
                                error.set(None);
                                editing.set(base);
                            },
                            onsave: save,
                            onforget: forget,
                            onremove,
                        }
                    }
                }
                form { class: "site-add", onsubmit: add,
                    input {
                        r#type: "url",
                        placeholder: "https://booru.example",
                        aria_label: t!("boorus-address"),
                        spellcheck: "false",
                        autocapitalize: "none",
                        value: "{address}",
                        oninput: move |e| address.set(e.value()),
                    }
                    Select {
                        aria_label: t!("boorus-api"),
                        value: kind().as_str().to_string(),
                        options: BooruApi::ALL.into_iter().map(|a| Choice::new(a.as_str(), a.name())).collect::<Vec<_>>(),
                        onchange: move |v: String| {
                            if let Some(api) = api_of(&v) {
                                kind.set(api);
                            }
                        },
                    }
                    button { class: "btn tonal", r#type: "submit", disabled: address.read().trim().is_empty(),
                        Ico { icon: Icon::Add }
                        {t!("add")}
                    }
                }
            }
        }
    }
}

/// One site in the sheet: its name, the account saved for it, and the two
/// boxes that take a new one.
#[component]
fn SiteRow(
    site: Site,
    account: Option<BooruAccount>,
    open: bool,
    login: Signal<String>,
    secret: Signal<String>,
    busy: bool,
    onedit: EventHandler<Option<String>>,
    onsave: EventHandler<RemoteSite>,
    onforget: EventHandler<i64>,
    onremove: EventHandler<RemoteSite>,
) -> Element {
    let (mut login, mut secret) = (login, secret);
    let api = site.site.api;
    let base = site.site.base.clone();
    let usable = account.clone().filter(|a| a.usable);
    let stale = account.is_some() && usable.is_none();
    let login_label = if api == BooruApi::Gelbooru {
        t!("boorus-user-id")
    } else {
        t!("boorus-login")
    };
    let key_label = if api == BooruApi::Moebooru {
        t!("boorus-password-hash")
    } else {
        t!("boorus-key")
    };
    let (kept, removed) = (site.site.clone(), site.site.clone());
    rsx! {
        li { class: "site-row",
            div { class: "site-line",
                span { class: "site-name", "{site.name}" }
                if site.custom {
                    span { class: "muted site-kind", "{api.name()}" }
                }
                if let Some(account) = usable {
                    span { class: "site-login",
                        Ico { icon: Icon::Check, size: 16 }
                        span { "{account.login}" }
                    }
                    button { class: "btn text small", r#type: "button", onclick: move |_| onforget.call(account.id),
                        {t!("boorus-account-forget")}
                    }
                } else {
                    span { class: "site-login",
                        if stale {
                            span { class: "pill bad", role: "status", {t!("boorus-account-again")} }
                        }
                    }
                    button {
                        class: "btn text small",
                        r#type: "button",
                        aria_expanded: open,
                        onclick: move |_| onedit.call((!open).then(|| base.clone())),
                        {t!("boorus-account")}
                    }
                }
                if site.custom {
                    button {
                        class: "icon-btn small danger",
                        r#type: "button",
                        title: t!("boorus-site-remove", site = site.name.clone()),
                        aria_label: t!("boorus-site-remove", site = site.name.clone()),
                        onclick: move |_| onremove.call(removed.clone()),
                        Ico { icon: Icon::Delete, size: 20 }
                    }
                }
            }
            if open {
                form {
                    class: "site-account",
                    onsubmit: move |e: FormEvent| {
                        e.prevent_default();
                        onsave.call(kept.clone());
                    },
                    input {
                        placeholder: login_label.clone(),
                        aria_label: login_label,
                        autocomplete: "off",
                        spellcheck: "false",
                        autocapitalize: "none",
                        value: "{login}",
                        oninput: move |e| login.set(e.value()),
                    }
                    input {
                        r#type: "password",
                        placeholder: key_label.clone(),
                        aria_label: key_label,
                        autocomplete: "off",
                        spellcheck: "false",
                        value: "{secret}",
                        oninput: move |e| secret.set(e.value()),
                    }
                    button { class: "btn", r#type: "submit", disabled: busy, {t!("save")} }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use starbooru_core::{Rating, TagCategory, Transfer};

    fn saved(api: &str, base: &str, login: &str) -> SavedBooru {
        SavedBooru {
            api: api.into(),
            base: base.into(),
            login: login.into(),
            api_key: format!("{login}-key"),
        }
    }

    fn site(api: BooruApi, base: &str) -> RemoteSite {
        RemoteSite {
            api,
            base: base.into(),
        }
    }

    fn preset(name: &str) -> RemoteSite {
        let found = SITES.into_iter().find(|s| s.name == name).unwrap();
        site(found.api, found.base)
    }

    fn post(base: &str, id: u64, md5: Option<&str>) -> RemotePost {
        RemotePost {
            id,
            site: base.into(),
            page_url: format!("{base}/posts/{id}"),
            preview_url: format!("{base}/preview/{id}.jpg"),
            sample_url: None,
            file_url: format!("{base}/file/{id}.jpg"),
            tags: vec!["sky".into(), "someone".into()],
            groups: Default::default(),
            rating: Rating::Sensitive,
            width: 640,
            height: 480,
            md5: md5.map(str::to_owned),
            source: None,
            score: None,
            created_at: None,
            file_size: None,
            post_id: None,
        }
    }

    fn job(id: i64, state: JobState, page: &str) -> ImportJob {
        ImportJob {
            id,
            url: format!("{page}.jpg"),
            source: Some(page.to_owned()),
            tags: Vec::new(),
            rating: Rating::default(),
            state,
            post_id: None,
            error: None,
            created_at: Utc::now(),
            finished_at: None,
            progress: None,
        }
    }

    const A: &str = "https://a.org";
    const B: &str = "https://b.org";

    #[test]
    fn remembers_added_sites_by_address() {
        let mut list = vec![saved("danbooru", "https://a.org", "a")];
        remember(&mut list, saved("gelbooru", "https://b.org", "b"));
        // Added again as another kind: the same place, and what an older
        // version kept with it stays until it has moved to the server.
        remember(&mut list, saved("blombooru", "https://a.org/", ""));
        assert_eq!(list.len(), 2);
        assert_eq!(
            (
                list[0].api.as_str(),
                list[0].base.as_str(),
                list[0].login.as_str()
            ),
            ("blombooru", "https://a.org", "a")
        );
        assert_eq!(list[1].base, "https://b.org", "the latest comes last");
        for i in 0..MAX_SAVED + 5 {
            remember(
                &mut list,
                saved("danbooru", &format!("https://{i}.org"), ""),
            );
        }
        assert_eq!(list.len(), MAX_SAVED);
        assert_eq!(
            list[MAX_SAVED - 1].base,
            format!("https://{}.org", MAX_SAVED + 4)
        );
        assert!(list.iter().all(|s| s.base != "https://a.org"));
    }

    #[test]
    fn accounts_kept_on_the_device_move_to_the_server() {
        let mut list = vec![
            saved("danbooru", "https://a.org", "a"),
            SavedBooru {
                api: "gelbooru".into(),
                base: "https://b.org".into(),
                ..SavedBooru::default()
            },
            // An API this version doesn't know can't be moved.
            saved("szurubooru", "https://c.org", "c"),
        ];
        let moving = kept_on_device(&list);
        assert_eq!(moving.len(), 1);
        assert_eq!(
            (moving[0].base.as_str(), moving[0].api_key.as_str()),
            ("https://a.org", "a-key")
        );
        forget_on_device(&mut list, "https://a.org/");
        assert_eq!((list[0].login.as_str(), list[0].api_key.as_str()), ("", ""));
        assert!(kept_on_device(&list).is_empty());
        // The site itself is still remembered.
        assert_eq!(list[0].base, "https://a.org");
        assert_eq!(tidy(" https://a.org// "), "https://a.org");
    }

    #[test]
    fn moving_an_account_off_the_device_writes_while_nothing_reads() {
        // Regression: the loop read the preferences for as long as it ran, and
        // taking the first account off the device, which writes to them,
        // crashed the page.
        use futures_util::FutureExt;
        use std::cell::RefCell;
        thread_local! {
            static LEFT: RefCell<Option<Vec<SavedBooru>>> = const { RefCell::new(None) };
        }
        fn app() -> Element {
            use_hook(|| {
                let prefs = Signal::new(Prefs {
                    boorus: vec![
                        saved("danbooru", "https://a.org", "a"),
                        saved("gelbooru", "https://b.org", "b"),
                        saved("e621", "https://c.org", "c"),
                    ],
                    ..Prefs::default()
                });
                // The server takes the first two.
                let save =
                    |account: SaveBooruAccount| async move { account.base != "https://c.org" };
                let done = move_accounts(prefs, save).now_or_never();
                assert!(done.is_some(), "nothing here waits");
                LEFT.with(|left| *left.borrow_mut() = Some(prefs.peek().boorus.clone()));
            });
            rsx! {}
        }
        let mut dom = dioxus::core::VirtualDom::new(app);
        dom.rebuild_in_place();
        let left = LEFT.with(|left| left.borrow_mut().take()).unwrap();
        let kept: Vec<(&str, &str)> = left
            .iter()
            .map(|s| (s.login.as_str(), s.api_key.as_str()))
            .collect();
        // The one the server refused stays, to try again the next time.
        assert_eq!(kept, [("", ""), ("", ""), ("c", "c-key")]);
        assert_eq!(left.len(), 3, "the sites themselves stay");
    }

    #[test]
    fn extra_tags_come_first_and_fit() {
        let post = vec!["sky".to_owned(), "sea".to_owned()];
        assert_eq!(
            import_tags(&post, "From_Danbooru sky").unwrap(),
            ["from_danbooru", "sky", "sea"]
        );
        let full: Vec<String> = (0..MAX_TAGS_PER_POST).map(|i| format!("t{i}")).collect();
        let tags = import_tags(&full, "mine").unwrap();
        assert_eq!(tags.len(), MAX_TAGS_PER_POST);
        assert_eq!(tags[0], "mine");
        assert!(import_tags(&post, "~bad").is_err());
    }

    #[test]
    fn a_post_is_imported_by_its_file_or_else_by_its_page() {
        let mut p = post(A, 7, None);
        p.groups.put(TagCategory::Artist, "someone");
        let request = import_request(&p, "mine").unwrap();
        assert_eq!(request.url, "https://a.org/file/7.jpg");
        assert_eq!(request.source.as_deref(), Some("https://a.org/posts/7"));
        assert_eq!(request.rating, Rating::Sensitive);
        // The user's tags first, then the post's with their categories.
        assert_eq!(request.tags, ["mine", "artist:someone", "sky"]);

        // Gelbooru without a key shows no file: the server reads the page.
        p.file_url = String::new();
        let request = import_request(&p, "").unwrap();
        assert_eq!(request.url, "https://a.org/posts/7");
        assert_eq!(request.source, None);
        assert_eq!(request.tags, ["artist:someone", "sky"]);

        assert!(import_request(&p, "~bad").is_err());
    }

    #[test]
    fn sites_go_into_an_address_and_come_back() {
        for preset in SITES {
            let site = site(preset.api, preset.base);
            let key = site_key(&site);
            assert_eq!(key, preset.name.to_lowercase());
            assert!(!key.contains([',', '|', ' ']), "{key}");
            assert_eq!(site_of(&key), Some(site));
        }
        assert_eq!(site_key(&preset("yande.re")), "yande.re");
        // A trailing slash is the same site, and so is another case.
        assert_eq!(
            site_key(&site(BooruApi::Danbooru, "https://danbooru.donmai.us/")),
            "danbooru"
        );
        assert_eq!(site_of(" Danbooru "), Some(preset("Danbooru")));

        // One added by its address, and one whose address has a comma.
        let nas = site(BooruApi::Blombooru, "http://nas:8000/booru");
        assert_eq!(site_key(&nas), "blombooru|http://nas:8000/booru");
        let odd = site(BooruApi::Danbooru, "https://x.org/a,b&c");
        assert_eq!(site_key(&odd), "danbooru|https://x.org/a~2Cb~26c");
        let all = vec![preset("Danbooru"), nas.clone(), odd.clone(), preset("e926")];
        let s = sites_param(&all);
        assert_eq!(
            s,
            "danbooru,blombooru|http://nas:8000/booru,danbooru|https://x.org/a~2Cb~26c,e926"
        );
        assert_eq!(parse_sites(&s), all);
        // A preset's address under another kind is a site of its own.
        let other = site(BooruApi::Danbooru, "https://safebooru.org");
        assert_eq!(site_key(&other), "danbooru|https://safebooru.org");
        assert_eq!(site_of(&site_key(&other)), Some(other));
    }

    #[test]
    fn what_an_address_carries_comes_back_as_it_went_in() {
        use std::str::FromStr;
        let texts = [
            "",
            "blue_sky rating:safe",
            "https://gelbooru.com/index.php?page=post&s=view&id=7#comments",
            "panty_&_stocking_with_garterbelt 100% c++ ~either a,b",
            "già_visto ★ 日本語",
            "~7E ~zz ~",
        ];
        let sites = sites_param(&[
            preset("yande.re"),
            site(BooruApi::Blombooru, "http://nas:8000/a,b&c%20d"),
        ]);
        for text in texts {
            let safe = to_address(text);
            assert!(!safe.contains(['&', '%', '#', '+', ',']), "{safe}");
            assert_eq!(from_address(&safe), text);
            // Through the router and back, as a reload or a shared link does.
            let post = Route::BooruPost {
                u: safe.clone(),
                q: safe.clone(),
                s: sites.clone(),
            };
            assert_eq!(
                Route::from_str(&post.to_string()).ok(),
                Some(post),
                "{text}"
            );
            let grid = search_route(text, &sites);
            let back = Route::from_str(&grid.to_string()).ok();
            assert_eq!(back, Some(grid.clone()), "{text}");
            let Route::Boorus { q, s } = grid else {
                panic!("not the grid");
            };
            assert_eq!(from_address(&q), text);
            assert_eq!(parse_sites(&s).len(), 2);
        }
        // Text nobody encoded reads as it is.
        assert_eq!(from_address("blue_sky ~a"), "blue_sky ~a");
    }

    #[test]
    fn an_address_names_each_site_once_and_only_ones_it_can_read() {
        assert!(parse_sites("").is_empty());
        assert_eq!(
            parse_sites(
                "danbooru,,nonsense,szurubooru|https://s.org,danbooru|not-an-address,e926,danbooru"
            ),
            [preset("Danbooru"), preset("e926")]
        );
        let many: Vec<String> = (0..MAX_SEARCH_SITES + 3)
            .map(|i| format!("danbooru|https://{i}.org"))
            .collect();
        assert_eq!(parse_sites(&many.join(",")).len(), MAX_SEARCH_SITES);
    }

    #[test]
    fn a_search_goes_to_the_sites_chosen_last_unless_the_address_says() {
        let first = SITES.first().unwrap();
        assert_eq!(chosen_sites("", &[]), [site(first.api, first.base)]);
        let last = vec!["e926".to_owned(), "gelbooru".to_owned()];
        assert_eq!(
            chosen_sites("", &last),
            [preset("e926"), preset("Gelbooru")]
        );
        assert_eq!(chosen_sites("konachan", &last), [preset("Konachan")]);
        // Nothing readable in the address, or in what was kept.
        assert_eq!(
            chosen_sites("??", &last),
            [preset("e926"), preset("Gelbooru")]
        );
        assert_eq!(
            chosen_sites("", &["gone".to_owned()]),
            [site(first.api, first.base)]
        );
    }

    #[test]
    fn the_first_chosen_site_completes_tags() {
        let first = |s: &str, last: &[String]| chosen_sites(s, last).into_iter().next();
        assert_eq!(first("gelbooru,danbooru", &[]), Some(preset("Gelbooru")));
        assert_eq!(
            first("", &["yande.re".to_owned(), "danbooru".to_owned()]),
            Some(preset("yande.re"))
        );
        assert_eq!(first("", &[]), Some(preset("Danbooru")));
    }

    #[test]
    fn chips_are_the_presets_then_the_added_sites() {
        let added = vec![
            saved("danbooru", "http://nas:8000/", ""),
            // The same as a preset, an unknown kind, and a bad address.
            saved("gelbooru", "https://gelbooru.com", ""),
            saved("szurubooru", "https://s.org", ""),
            saved("danbooru", "nas", ""),
        ];
        let linked = site(BooruApi::E621, "https://e621.net");
        let chips = all_sites(&added, &[preset("Danbooru"), linked.clone()]);
        assert_eq!(chips.len(), SITES.len() + 2);
        assert!(chips.iter().take(SITES.len()).all(|c| !c.custom));
        let nas = &chips[SITES.len()];
        assert_eq!((nas.name.as_str(), nas.custom), ("nas:8000", true));
        assert_eq!(nas.site, site(BooruApi::Danbooru, "http://nas:8000"));
        // A site only the address names has a chip, with nothing to remove.
        let other = &chips[SITES.len() + 1];
        assert_eq!((other.name.as_str(), other.custom), ("e621.net", false));
        assert_eq!(other.site, linked);
        assert_eq!(site_name("https://yande.re/"), "yande.re");
        assert_eq!(site_name("http://127.0.0.1:9001"), "127.0.0.1:9001");
    }

    #[test]
    fn a_chip_turns_its_site_on_or_off_and_one_stays_on() {
        let chips = all_sites(&[], &[]);
        let (dan, gel, e9) = (preset("Danbooru"), preset("Gelbooru"), preset("e926"));
        let one = vec![dan.clone()];
        assert_eq!(toggled(&one, &dan, &chips), one, "the last one stays");
        // In the order of the chips, whatever the order of the clicks.
        let two = toggled(&one, &e9, &chips);
        assert_eq!(two, [dan.clone(), e9.clone()]);
        let three = toggled(&two, &gel, &chips);
        assert_eq!(three, [dan.clone(), gel.clone(), e9.clone()]);
        assert_eq!(toggled(&three, &dan, &chips), [gel, e9]);
        // No more than a search goes to.
        let full: Vec<RemoteSite> = (0..MAX_SEARCH_SITES)
            .map(|i| site(BooruApi::Danbooru, &format!("https://{i}.org")))
            .collect();
        assert_eq!(toggled(&full, &dan, &chips).len(), MAX_SEARCH_SITES);
    }

    fn pages(all: &[Rc<RemotePost>]) -> Vec<&str> {
        all.iter().map(|p| p.page_url.as_str()).collect()
    }

    #[test]
    fn a_page_of_results_takes_a_post_from_each_site_in_turn() {
        let a = vec![post(A, 1, None), post(A, 2, None), post(A, 3, None)];
        let b = vec![post(B, 1, None)];
        let mixed = interleave(vec![a, Vec::new(), b]);
        let order: Vec<&str> = mixed.iter().map(|p| p.page_url.as_str()).collect();
        assert_eq!(
            order,
            [
                "https://a.org/posts/1",
                "https://b.org/posts/1",
                "https://a.org/posts/2",
                "https://a.org/posts/3"
            ]
        );
        assert!(interleave(Vec::new()).is_empty());

        // The next page goes after it, without what is already there.
        let mut all = Vec::new();
        assert_eq!(append(&mut all, mixed), 4);
        let more = vec![post(A, 3, None), post(A, 4, None), post(A, 4, None)];
        assert_eq!(append(&mut all, more), 1);
        assert_eq!(pages(&all).len(), 5);
        assert_eq!(pages(&all)[4], "https://a.org/posts/4");
    }

    #[test]
    fn the_same_file_on_two_sites_shows_once_when_asked() {
        let all: Vec<Rc<RemotePost>> = [
            post(A, 1, Some("aa")),
            post(B, 9, Some("bb")),
            post(A, 2, None),
            post(B, 8, Some("aa")),
            post(B, 7, None),
            post(A, 3, Some("bb")),
            post(A, 4, Some("")),
            post(B, 6, Some("")),
        ]
        .into_iter()
        .map(Rc::new)
        .collect();

        // As they came, when duplicates aren't hidden.
        let every = shown(&all, false);
        assert_eq!(every.len(), all.len());
        assert!(every.iter().all(|f| f.also.is_empty()));

        let once = shown(&all, true);
        let order: Vec<&str> = once.iter().map(|f| f.post.page_url.as_str()).collect();
        assert_eq!(
            order,
            [
                "https://a.org/posts/1",
                "https://b.org/posts/9",
                "https://a.org/posts/2",
                "https://b.org/posts/7",
                // No MD5, or an empty one, is never a duplicate.
                "https://a.org/posts/4",
                "https://b.org/posts/6"
            ]
        );
        // The one that stays knows where else the file is.
        assert_eq!(
            once[0].also,
            [Twin {
                site: B.into(),
                page_url: "https://b.org/posts/8".into()
            }]
        );
        assert_eq!(once[1].also[0].page_url, "https://a.org/posts/3");
        assert!(once[2].also.is_empty());
    }

    #[test]
    fn a_post_says_how_saving_it_stands() {
        let mut p = post(A, 1, None);
        let page = p.page_url.clone();
        let jobs = |job: ImportJob| activity::jobs_by_source(&[job]);
        assert_eq!(status_of(&p, &HashMap::new()), None);

        let mut running = job(1, JobState::Running, &page);
        assert_eq!(
            status_of(&p, &jobs(running.clone())),
            Some(Status::Busy(None))
        );
        running.progress = Some(Transfer {
            done: 42,
            total: Some(100),
            speed: 0,
        });
        assert_eq!(status_of(&p, &jobs(running)), Some(Status::Busy(Some(42))));
        assert_eq!(
            status_of(&p, &jobs(job(1, JobState::Queued, &page))),
            Some(Status::Busy(None))
        );

        let mut done = job(1, JobState::Done, &page);
        done.post_id = Some(12);
        assert_eq!(
            status_of(&p, &jobs(done.clone())),
            Some(Status::Saved(Some(12), String::new()))
        );
        let mut failed = job(2, JobState::Failed, &page);
        failed.error = Some("the site answered 404".into());
        assert_eq!(
            status_of(&p, &jobs(failed.clone())),
            Some(Status::Failed("the site answered 404".into()))
        );
        let mut twin = job(3, JobState::Duplicate, &page);
        twin.error = Some("already here as post 5".into());
        assert_eq!(
            status_of(&p, &jobs(twin)),
            Some(Status::Saved(None, "already here as post 5".into()))
        );

        // A post the server already knows is saved, whatever an old job says.
        p.post_id = Some(30);
        assert_eq!(
            status_of(&p, &HashMap::new()),
            Some(Status::Saved(Some(30), String::new()))
        );
        assert_eq!(
            status_of(&p, &jobs(failed)),
            Some(Status::Saved(Some(30), String::new()))
        );
        // One imported by its page is found by that page.
        let mut by_page = job(4, JobState::Queued, &page);
        by_page.url = page.clone();
        by_page.source = None;
        assert_eq!(status_of(&p, &jobs(by_page)), Some(Status::Busy(None)));

        // Once done, the post keeps the note after the job is forgotten.
        let mut all = vec![Rc::new(post(A, 1, None)), Rc::new(post(A, 2, None))];
        assert!(mark_saved(&mut all, &jobs(done.clone())));
        assert_eq!((all[0].post_id, all[1].post_id), (Some(12), None));
        assert!(!mark_saved(&mut all, &jobs(done)));
    }

    #[test]
    fn a_saved_search_is_named_and_matched_by_its_sites() {
        let (dan, gel) = (preset("Danbooru"), preset("Gelbooru"));
        assert_eq!(
            search_name("blue_sky rating:safe", &[dan.clone()]),
            "blue_sky rating:safe"
        );
        assert_eq!(
            search_name("", &[dan.clone(), gel.clone()]),
            "Danbooru, Gelbooru"
        );
        let long = "x".repeat(MAX_SEARCH_NAME_CHARS + 10);
        assert_eq!(
            search_name(&long, &[]).chars().count(),
            MAX_SEARCH_NAME_CHARS
        );
        assert!(same_sites(
            &[dan.clone(), gel.clone()],
            &[gel.clone(), dan.clone()]
        ));
        assert!(!same_sites(&[dan.clone()], &[dan, gel]));
        assert_eq!(tidy_query("  a   b "), "a b");
    }

    #[test]
    fn pictures_behind_the_server_get_its_address() {
        let client = Client::new("https://my.example/").unwrap();
        assert_eq!(
            picture_src(&client, "/api/v1/remote/media?u=x&exp=1&sig=ab"),
            "https://my.example/api/v1/remote/media?u=x&exp=1&sig=ab"
        );
        assert_eq!(
            picture_src(&client, "https://cdn.d.org/360x360/a.jpg"),
            "https://cdn.d.org/360x360/a.jpg"
        );
    }

    #[test]
    fn a_picture_without_a_size_gets_a_square_tile() {
        let mut p = post(A, 1, None);
        assert_eq!(size_of(&p), (Some(640), Some(480)));
        p.width = 0;
        assert_eq!(size_of(&p), (None, Some(480)));
        p.height = u32::MAX;
        assert_eq!(size_of(&p), (None, None));
    }

    #[test]
    fn a_site_is_asked_again_only_while_it_has_more_and_answers() {
        let mut page = SitePage::first(&preset("Danbooru"));
        assert!(page.wanted());
        assert_eq!(page.next, 1);
        page.error = Some("that site is busy".into());
        assert!(!page.wanted());
        page.error = None;
        page.more = false;
        assert!(!page.wanted());
    }

    #[test]
    fn the_grid_is_the_gallerys() {
        // The same markup and the same scripts, so the two pages scroll, lay
        // out and load alike.
        let source = include_str!("boorus.rs");
        let (page, _) = source.split_once("\n#[cfg(test)]").unwrap();
        for part in [
            "class: \"masonry\",",
            "\"masonry-box selecting\"",
            "document::eval(WATCH_WIDTH)",
            "document::eval(WATCH_NEAR)",
            "document::eval(WATCH_SCROLL)",
            "ViewMenu { width, gap }",
        ] {
            assert!(page.contains(part), "{part}");
        }
        // Tiles carry no `data-post`: that is how a picture flying back from
        // the viewer finds its tile in the gallery (motion.rs).
        assert!(!page.contains("\"data-post\""));
        // Keys never stay in the app: an account goes to the server.
        assert!(page.contains("client.save_booru_account(&account)"));
        assert!(!page.contains("api_key: Some("));
    }
}
