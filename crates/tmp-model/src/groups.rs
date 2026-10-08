//! Processes page logic, lifted from `lite/processes.rs` so that every port
//! groups, sorts and selects processes the same way.
//!
//! The order is the same as lite's, with one addition: children within a group
//! break CPU ties by PID, so child rows don't swap places between polls.

use std::cmp::Ordering;
use std::collections::HashMap;

use crate::types::Proc;

/// A sortable column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SortKey {
    Name,
    Cpu,
    Memory,
    Gpu,
    Disk,
    Net,
}

impl SortKey {
    /// Columns, left to right.
    pub const COLUMNS: [SortKey; 6] = [
        SortKey::Name,
        SortKey::Cpu,
        SortKey::Memory,
        SortKey::Gpu,
        SortKey::Disk,
        SortKey::Net,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SortKey::Name => "Name",
            SortKey::Cpu => "CPU",
            SortKey::Memory => "Memory",
            SortKey::Gpu => "GPU",
            SortKey::Disk => "Disk",
            SortKey::Net => "Network",
        }
    }
}

/// Current sort column and direction. The default is CPU, descending.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sort {
    pub key: SortKey,
    pub ascending: bool,
}

impl Default for Sort {
    fn default() -> Self {
        Self {
            key: SortKey::Cpu,
            ascending: false,
        }
    }
}

impl Sort {
    /// Header click. Clicking the active column flips its direction. A new
    /// column starts ascending for Name and descending for numbers.
    pub fn click(&mut self, key: SortKey) {
        if self.key == key {
            self.ascending = !self.ascending;
        } else {
            self.key = key;
            self.ascending = key == SortKey::Name;
        }
    }
}

/// Processes that share a name, with aggregate metrics.
#[derive(Clone, Debug)]
pub struct Group<'a> {
    pub name: &'a str,
    /// Highest CPU first.
    pub procs: Vec<&'a Proc>,
    pub cpu: f64,
    /// MB
    pub mem: f64,
    pub gpu: f64,
    /// Bytes/s
    pub disk: f64,
    /// Bytes/s
    pub net: f64,
}

impl Group<'_> {
    /// True when the group has more than one instance, so its row gets an expander and a count.
    pub fn is_multi(&self) -> bool {
        self.procs.len() > 1
    }

    /// What clicking this row selects: the whole group, or its single PID.
    pub fn selection(&self) -> Selection {
        if self.is_multi() {
            Selection::Group(self.name.to_string())
        } else {
            Selection::Pid(self.procs[0].pid)
        }
    }
}

/// Filter by name, group by name, add up each group's metrics, then sort.
pub fn build_groups<'a>(procs: &'a [Proc], filter: &str, sort: Sort) -> Vec<Group<'a>> {
    let needle = filter.to_lowercase();
    let mut by_name: HashMap<&str, Group<'a>> = HashMap::new();
    for p in procs {
        if !needle.is_empty() && !p.name.to_lowercase().contains(&needle) {
            continue;
        }
        let g = by_name.entry(p.name.as_str()).or_insert_with(|| Group {
            name: &p.name,
            procs: Vec::new(),
            cpu: 0.0,
            mem: 0.0,
            gpu: 0.0,
            disk: 0.0,
            net: 0.0,
        });
        g.procs.push(p);
        g.cpu += p.cpu_percent;
        g.mem += p.memory_mb;
        g.gpu += p.gpu_percent;
        g.disk += p.disk_bps;
        g.net += p.net_bps;
    }
    let mut groups: Vec<Group<'a>> = by_name.into_values().collect();

    // Deterministic total order: numeric direction first, then name ascending
    // as a tiebreak. Without the tiebreak, the many groups at 0 fall back to
    // HashMap order, which changes every poll and makes rows visibly churn.
    let asc = sort.ascending;
    let by_name = |a: &Group, b: &Group| a.name.to_lowercase().cmp(&b.name.to_lowercase());
    let num = |x: f64, y: f64, a: &Group, b: &Group| {
        let o = x.partial_cmp(&y).unwrap_or(Ordering::Equal);
        let o = if asc { o } else { o.reverse() };
        o.then_with(|| by_name(a, b))
    };
    match sort.key {
        SortKey::Name => groups.sort_by(|a, b| {
            let o = by_name(a, b);
            if asc { o } else { o.reverse() }
        }),
        SortKey::Cpu => groups.sort_by(|a, b| num(a.cpu, b.cpu, a, b)),
        SortKey::Memory => groups.sort_by(|a, b| num(a.mem, b.mem, a, b)),
        SortKey::Gpu => groups.sort_by(|a, b| num(a.gpu, b.gpu, a, b)),
        SortKey::Disk => groups.sort_by(|a, b| num(a.disk, b.disk, a, b)),
        SortKey::Net => groups.sort_by(|a, b| num(a.net, b.net, a, b)),
    }
    for g in &mut groups {
        g.procs.sort_by(|a, b| {
            b.cpu_percent
                .partial_cmp(&a.cpu_percent)
                .unwrap_or(Ordering::Equal)
                .then(a.pid.cmp(&b.pid))
        });
    }
    groups
}

/// One row of the virtualized table body.
#[derive(Clone, Copy, Debug)]
pub enum Row<'a> {
    /// Index into the groups slice.
    Group(usize),
    /// One instance under an expanded group.
    Child(&'a Proc),
}

/// Flatten groups into table rows. Each expanded group with more than one
/// instance is followed by its children.
pub fn flatten<'a>(groups: &[Group<'a>], is_expanded: impl Fn(&str) -> bool) -> Vec<Row<'a>> {
    let mut rows = Vec::with_capacity(groups.len());
    for (i, g) in groups.iter().enumerate() {
        rows.push(Row::Group(i));
        if g.is_multi() && is_expanded(g.name) {
            rows.extend(g.procs.iter().map(|p| Row::Child(p)));
        }
    }
    rows
}

/// The selected row.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Selection {
    Group(String),
    Pid(u32),
}

impl Selection {
    /// The PIDs to act on, such as for End task. They are resolved against the
    /// groups currently shown, so ending a group ends exactly the instances on
    /// screen.
    pub fn pids(&self, groups: &[Group<'_>]) -> Vec<u32> {
        match self {
            Selection::Pid(pid) => vec![*pid],
            Selection::Group(name) => groups
                .iter()
                .find(|g| g.name == name)
                .map(|g| g.procs.iter().map(|p| p.pid).collect())
                .unwrap_or_default(),
        }
    }
}

/// Friendly label for a multi-process role slug.
pub fn type_label(slug: &str) -> &str {
    match slug {
        "renderer" => "Tab",
        "gpu" => "GPU",
        "utility" => "Utility",
        "utility-video" => "Video",
        "utility-audio" => "Audio",
        "utility-storage" => "Storage",
        "utility-network" => "Network",
        "crashpad" => "Crash Handler",
        "socket" => "Socket",
        "rdd" => "Media",
        "content" => "Content",
        "shared" => "Shared Process",
        "watcher" => "File Watcher",
        "pty-host" => "Terminal",
        "extension-host" => "Extension Host",
        "extension" => "Extension",
        "main" => "Main",
        "service" => "Service",
        other => other,
    }
}

/// What a child row shows: the role label and the window title, truncated to
/// 48 characters. Lite renders them as "Role — title", "Role", "title", or a
/// muted "Process" when both are missing.
pub fn child_label(p: &Proc) -> (Option<&str>, String) {
    let role = p.process_type.as_deref().map(type_label);
    (role, crate::fmt::truncate(&p.window_title, 48))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pid: u32, name: &str, cpu: f64) -> Proc {
        Proc {
            pid,
            name: name.into(),
            memory_mb: 10.0,
            cpu_percent: cpu,
            gpu_percent: 0.0,
            disk_bps: 0.0,
            net_bps: 0.0,
            process_type: None,
            window_title: String::new(),
        }
    }

    fn names<'a>(groups: &'a [Group<'a>]) -> Vec<&'a str> {
        groups.iter().map(|g| g.name).collect()
    }

    #[test]
    fn groups_aggregate_and_sort_with_name_tiebreak() {
        let procs = vec![
            p(8, "Zed", 0.0),
            p(4, "app", 1.0),
            p(12, "App", 0.0),
            p(16, "app", 3.0),
            p(20, "Bee", 0.0),
        ];
        let groups = build_groups(&procs, "", Sort::default());
        // "app" (4.0) first; then the 0% ties alphabetically, case-insensitive.
        assert_eq!(names(&groups), ["app", "App", "Bee", "Zed"]);
        assert_eq!(groups[0].cpu, 4.0);
        assert_eq!(groups[0].mem, 20.0);
        assert_eq!(
            groups[0].procs.iter().map(|p| p.pid).collect::<Vec<_>>(),
            [16, 4]
        );
    }

    #[test]
    fn children_break_cpu_ties_by_pid() {
        let procs = vec![p(40, "a", 1.0), p(8, "a", 1.0), p(24, "a", 1.0)];
        let groups = build_groups(&procs, "", Sort::default());
        assert_eq!(
            groups[0].procs.iter().map(|p| p.pid).collect::<Vec<_>>(),
            [8, 24, 40]
        );
    }

    #[test]
    fn filter_is_case_insensitive_substring() {
        let procs = vec![p(4, "Google Chrome", 0.0), p(8, "Discord", 0.0)];
        let groups = build_groups(&procs, "CHROME", Sort::default());
        assert_eq!(names(&groups), ["Google Chrome"]);
    }

    #[test]
    fn name_sort_descending_reverses() {
        let procs = vec![p(4, "b", 0.0), p(8, "a", 9.0), p(12, "c", 0.0)];
        let sort = Sort {
            key: SortKey::Name,
            ascending: false,
        };
        assert_eq!(names(&build_groups(&procs, "", sort)), ["c", "b", "a"]);
    }

    #[test]
    fn header_click_rules() {
        let mut s = Sort::default();
        s.click(SortKey::Cpu);
        assert_eq!(
            s,
            Sort {
                key: SortKey::Cpu,
                ascending: true
            }
        );
        s.click(SortKey::Name);
        assert_eq!(
            s,
            Sort {
                key: SortKey::Name,
                ascending: true
            }
        );
        s.click(SortKey::Memory);
        assert_eq!(
            s,
            Sort {
                key: SortKey::Memory,
                ascending: false
            }
        );
    }

    #[test]
    fn flatten_expands_only_multi_groups() {
        let procs = vec![p(4, "a", 2.0), p(8, "a", 1.0), p(12, "b", 0.0)];
        let groups = build_groups(&procs, "", Sort::default());
        let rows = flatten(&groups, |_| true);
        assert_eq!(rows.len(), 4); // a, a/4, a/8, b (single, never expands)
        assert!(matches!(rows[1], Row::Child(c) if c.pid == 4));
        assert_eq!(flatten(&groups, |_| false).len(), 2);
    }

    #[test]
    fn selection_resolves_pids_against_visible_groups() {
        let procs = vec![p(4, "a", 0.0), p(8, "a", 0.0), p(12, "b", 0.0)];
        let groups = build_groups(&procs, "", Sort::default());
        assert_eq!(groups[0].selection(), Selection::Group("a".into()));
        let mut pids = Selection::Group("a".into()).pids(&groups);
        pids.sort();
        assert_eq!(pids, [4, 8]);
        assert_eq!(groups[1].selection(), Selection::Pid(12));
        assert!(Selection::Group("gone".into()).pids(&groups).is_empty());
    }

    #[test]
    fn child_label_maps_roles_and_truncates_titles() {
        let mut c = p(4, "Google Chrome", 0.0);
        c.process_type = Some("renderer".into());
        c.window_title = "x".repeat(60);
        let (role, title) = child_label(&c);
        assert_eq!(role, Some("Tab"));
        assert_eq!(title.chars().count(), 48);
        assert!(title.ends_with('…'));
    }
}
