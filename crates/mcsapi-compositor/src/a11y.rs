//! One accessibility tree for the whole desktop.
//!
//! egui builds an [AccessKit](accesskit) tree for every context it runs: the
//! chrome and each in-process app. Wayland clients publish their own trees
//! on the AT-SPI bus, and the shell can add nodes egui never sees, such as
//! the buttons it paints on title bars or a subtree an out-of-process
//! program registered ([`Shell::access_subtrees`](crate::Shell::access_subtrees)).
//!
//! The host's `Merger` joins them under one root, one node per window, with node IDs
//! of its own: egui IDs are hashes that two contexts could share, and an
//! agent needs an ID that stays put while the element exists. The merged
//! tree goes to screen readers over AT-SPI (feature `atspi`) and, flattened
//! into a [`Snapshot`], to the shell for agents.

use std::collections::{HashMap, HashSet};

use accesskit::{Action, Node, NodeId, Role, Toggled, Tree, TreeId, TreeUpdate};
use mcsapi::{Geometry, WindowId};

/// Nodes the shell adds under a window, in its own ID space.
#[derive(Clone, Debug, PartialEq)]
pub struct Subtree {
    /// The window the nodes belong to.
    pub window: WindowId,
    /// The subtree's top nodes, shown as children of the window.
    pub roots: Vec<NodeId>,
    /// Every node, keyed by an ID unique within this subtree.
    pub nodes: Vec<(NodeId, Node)>,
    /// [`Origin::Shell`] for nodes the shell draws itself, [`Origin::App`]
    /// for nodes a program registered, whose text the program chose.
    pub origin: Origin,
}

/// Where an element in a [`Snapshot`] comes from, which says how far its
/// text can be trusted.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum Origin {
    /// The desktop root.
    Desktop,
    /// The shell's chrome (bars, overlays, palette).
    Chrome,
    /// A window, as the compositor knows it. Its name is the window title,
    /// which the app chose.
    Window,
    /// Content of an in-process app.
    App,
    /// Nodes the shell added through a [`Subtree`] for what it draws.
    Shell,
}

/// One element of a [`Snapshot`].
#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    /// Stable while the element exists; pass it to
    /// [`Command::Act`](crate::Command::Act).
    pub id: u64,
    /// Parent element, `None` for the root.
    pub parent: Option<u64>,
    /// What kind of element this is.
    pub role: Role,
    /// Accessible name (label, title or text).
    pub name: String,
    /// Current value, for text fields, sliders and the like.
    pub value: Option<String>,
    /// Longer description or tooltip.
    pub description: Option<String>,
    /// Bounds in logical, output-relative coordinates.
    pub bounds: Option<Geometry>,
    /// The window the element is in.
    pub window: Option<WindowId>,
    /// Where it comes from.
    pub origin: Origin,
    /// Whether it has keyboard focus.
    pub focused: bool,
    /// Whether it is disabled.
    pub disabled: bool,
    /// Checked state of checkboxes and toggles.
    pub toggled: Option<bool>,
    /// Selected state of list items, tabs and the like.
    pub selected: Option<bool>,
    /// Expanded state of menus and disclosures.
    pub expanded: Option<bool>,
    /// Actions it supports.
    pub actions: Vec<Action>,
    /// Child elements.
    pub children: Vec<u64>,
}

/// The desktop's accessibility tree, flattened for agents. Containers with
/// no name and no actions are left out and their children lifted.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    /// Elements in reading order, parents before children.
    pub elements: Vec<Element>,
    /// The focused element.
    pub focus: Option<u64>,
}

impl Snapshot {
    /// An element by ID.
    pub fn get(&self, id: u64) -> Option<&Element> {
        self.elements.iter().find(|e| e.id == id)
    }
}

/// A window as the merger sees it.
pub(crate) struct WindowInfo<'a> {
    pub window: WindowId,
    pub title: &'a str,
    pub app_id: &'a str,
    pub frame: Geometry,
    pub focused: bool,
    /// The in-process app's egui tree.
    pub content: Option<&'a TreeUpdate>,
}

/// Which tree a merged node came from, and its ID there.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum Source {
    Root,
    Chrome(NodeId),
    Window(WindowId),
    App(WindowId, NodeId),
    Shell(WindowId, NodeId, Origin),
}

/// Joins the chrome, the windows and their contents into one tree with
/// stable IDs.
#[derive(Debug)]
pub(crate) struct Merger {
    ids: HashMap<Source, NodeId>,
    sources: HashMap<NodeId, Source>,
    next: u64,
    name: String,
}

impl Merger {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            ids: HashMap::new(),
            sources: HashMap::new(),
            next: 1,
            name: name.into(),
        }
    }

    /// Where a merged node came from.
    pub fn source(&self, id: NodeId) -> Option<Source> {
        self.sources.get(&id).copied()
    }

    fn id(&mut self, source: Source, seen: &mut HashSet<Source>) -> NodeId {
        seen.insert(source);
        if let Some(id) = self.ids.get(&source) {
            return *id;
        }
        let id = NodeId(self.next);
        self.next += 1;
        self.ids.insert(source, id);
        self.sources.insert(id, source);
        id
    }

    /// A full tree update. Windows come bottom to top, as placed; they are
    /// listed topmost first, after the chrome, which is read first.
    pub fn build(
        &mut self,
        chrome: Option<&TreeUpdate>,
        windows: &[WindowInfo<'_>],
        subtrees: &[Subtree],
    ) -> TreeUpdate {
        let mut seen = HashSet::new();
        let mut nodes = Vec::new();
        let root = self.id(Source::Root, &mut seen);
        let mut root_node = Node::new(Role::Window);
        root_node.set_label(self.name.clone());
        let mut focus = None;

        if let Some(chrome) = chrome {
            let children = self.graft(chrome, Source::Chrome, &mut nodes, &mut seen);
            root_node.set_children(children);
            if chrome.focus != tree_root(chrome) {
                focus = self.ids.get(&Source::Chrome(chrome.focus)).copied();
            }
        }
        for info in windows.iter().rev() {
            let w = info.window;
            let id = self.id(Source::Window(w), &mut seen);
            let mut node = Node::new(Role::Window);
            node.set_label(if info.title.is_empty() {
                info.app_id
            } else {
                info.title
            });
            node.set_description(info.app_id);
            node.set_bounds(rect(info.frame));
            node.add_action(Action::Focus);
            node.add_action(Action::Click);
            let mut children = Vec::new();
            if let Some(content) = info.content {
                children.extend(self.graft(content, |n| Source::App(w, n), &mut nodes, &mut seen));
                if info.focused && content.focus != tree_root(content) {
                    focus = focus.or_else(|| self.ids.get(&Source::App(w, content.focus)).copied());
                }
            }
            for subtree in subtrees.iter().filter(|s| s.window == w) {
                children.extend(self.graft_subtree(subtree, &mut nodes, &mut seen));
            }
            node.set_children(children);
            nodes.push((id, node));
            if info.focused {
                focus = focus.or(Some(id));
            }
            root_node.push_child(id);
        }
        nodes.push((root, root_node));

        // Forget nodes that are gone, so the maps don't grow forever.
        self.ids.retain(|source, _| seen.contains(source));
        let ids = &self.ids;
        self.sources.retain(|_, source| ids.contains_key(source));

        TreeUpdate {
            nodes,
            tree: Some(Tree {
                toolkit_name: Some("mcsapi".into()),
                toolkit_version: Some(env!("CARGO_PKG_VERSION").into()),
                ..Tree::new(root)
            }),
            tree_id: TreeId::ROOT,
            focus: focus.unwrap_or(root),
        }
    }

    /// Copies the nodes reachable from `update`'s root, renumbered, and
    /// returns the IDs of the root's children.
    fn graft(
        &mut self,
        update: &TreeUpdate,
        source: impl Fn(NodeId) -> Source,
        out: &mut Vec<(NodeId, Node)>,
        seen: &mut HashSet<Source>,
    ) -> Vec<NodeId> {
        let by_id: HashMap<NodeId, &Node> = update.nodes.iter().map(|(id, n)| (*id, n)).collect();
        let root = tree_root(update);
        let Some(top) = by_id.get(&root) else {
            return Vec::new();
        };
        let tops = top.children().to_vec();
        self.copy(&tops, &by_id, &source, out, seen)
    }

    fn graft_subtree(
        &mut self,
        subtree: &Subtree,
        out: &mut Vec<(NodeId, Node)>,
        seen: &mut HashSet<Source>,
    ) -> Vec<NodeId> {
        let by_id: HashMap<NodeId, &Node> = subtree.nodes.iter().map(|(id, n)| (*id, n)).collect();
        let (w, origin) = (subtree.window, subtree.origin);
        self.copy(
            &subtree.roots,
            &by_id,
            &|n| Source::Shell(w, n, origin),
            out,
            seen,
        )
    }

    /// Copies `ids` and everything under them, depth first.
    fn copy(
        &mut self,
        ids: &[NodeId],
        by_id: &HashMap<NodeId, &Node>,
        source: &dyn Fn(NodeId) -> Source,
        out: &mut Vec<(NodeId, Node)>,
        seen: &mut HashSet<Source>,
    ) -> Vec<NodeId> {
        let mut mapped = Vec::with_capacity(ids.len());
        let mut stack: Vec<NodeId> = ids.iter().rev().copied().collect();
        let mut visited = HashSet::new();
        let mut tops = ids.iter().copied().collect::<HashSet<_>>();
        while let Some(id) = stack.pop() {
            // A malformed tree could list a node twice or loop.
            if !visited.insert(id) {
                continue;
            }
            let Some(node) = by_id.get(&id) else {
                continue;
            };
            let new_id = self.id(source(id), seen);
            if tops.remove(&id) {
                mapped.push(new_id);
            }
            let mut node = (*node).clone();
            let children: Vec<NodeId> = node
                .children()
                .iter()
                .filter(|c| by_id.contains_key(c))
                .copied()
                .collect();
            node.set_children(
                children
                    .iter()
                    .map(|c| self.id(source(*c), seen))
                    .collect::<Vec<_>>(),
            );
            self.relink(&mut node, by_id, source, seen);
            out.push((new_id, node));
            stack.extend(children.iter().rev());
        }
        mapped
    }
}

impl Merger {
    /// Renumbers every reference a node holds to other nodes, dropping the
    /// ones that point outside its tree. A dangling text selection makes
    /// the AT-SPI adapter panic, so none may survive.
    fn relink(
        &mut self,
        node: &mut Node,
        by_id: &HashMap<NodeId, &Node>,
        source: &dyn Fn(NodeId) -> Source,
        seen: &mut HashSet<Source>,
    ) {
        let mut map = |id: NodeId| by_id.contains_key(&id).then(|| self.id(source(id), seen));
        macro_rules! many {
            ($($get:ident $set:ident),*) => {$(
                let ids: Vec<NodeId> = node.$get().iter().filter_map(|id| map(*id)).collect();
                node.$set(ids);
            )*};
        }
        many!(
            labelled_by set_labelled_by,
            described_by set_described_by,
            controls set_controls,
            details set_details,
            flow_to set_flow_to,
            owns set_owns,
            radio_group set_radio_group
        );
        macro_rules! one {
            ($($get:ident $set:ident $clear:ident),*) => {$(
                match node.$get().and_then(&mut map) {
                    Some(id) => node.$set(id),
                    None => node.$clear(),
                }
            )*};
        }
        one!(
            active_descendant set_active_descendant clear_active_descendant,
            error_message set_error_message clear_error_message,
            in_page_link_target set_in_page_link_target clear_in_page_link_target,
            member_of set_member_of clear_member_of,
            next_on_line set_next_on_line clear_next_on_line,
            previous_on_line set_previous_on_line clear_previous_on_line,
            popup_for set_popup_for clear_popup_for
        );
        if let Some(selection) = node.text_selection().copied() {
            match (map(selection.anchor.node), map(selection.focus.node)) {
                (Some(anchor), Some(focus)) => node.set_text_selection(accesskit::TextSelection {
                    anchor: accesskit::TextPosition {
                        node: anchor,
                        ..selection.anchor
                    },
                    focus: accesskit::TextPosition {
                        node: focus,
                        ..selection.focus
                    },
                }),
                _ => node.clear_text_selection(),
            }
        }
    }
}

fn tree_root(update: &TreeUpdate) -> NodeId {
    update.tree.as_ref().map_or(
        // egui always sends the tree; fall back to its first node.
        update.nodes.first().map_or(NodeId(0), |(id, _)| *id),
        |t| t.root,
    )
}

fn rect(g: Geometry) -> accesskit::Rect {
    accesskit::Rect {
        x0: f64::from(g.loc.x),
        y0: f64::from(g.loc.y),
        x1: f64::from(g.loc.x + g.size.w),
        y1: f64::from(g.loc.y + g.size.h),
    }
}

/// Flattens a merged tree for agents.
pub(crate) fn snapshot(merger: &Merger, update: &TreeUpdate) -> Snapshot {
    let by_id: HashMap<NodeId, &Node> = update.nodes.iter().map(|(id, n)| (*id, n)).collect();
    let Some(root) = update.tree.as_ref().map(|t| t.root) else {
        return Snapshot::default();
    };
    let mut snapshot = Snapshot::default();
    let mut index = HashMap::new();
    // (node, nearest kept ancestor, window)
    let mut stack = vec![(root, None::<u64>, None::<WindowId>)];
    let mut visited = HashSet::new();
    while let Some((id, parent, window)) = stack.pop() {
        if !visited.insert(id) {
            continue;
        }
        let Some(node) = by_id.get(&id) else {
            continue;
        };
        // Text runs repeat their label's text, word by word.
        if node.is_hidden() || node.role() == Role::TextRun {
            continue;
        }
        let source = merger.source(id);
        let window = match source {
            Some(Source::Window(w) | Source::App(w, _) | Source::Shell(w, ..)) => Some(w),
            _ => window,
        };
        let origin = match source {
            Some(Source::Root) | None => Origin::Desktop,
            Some(Source::Chrome(_)) => Origin::Chrome,
            Some(Source::Window(_)) => Origin::Window,
            Some(Source::App(..)) => Origin::App,
            Some(Source::Shell(_, _, origin)) => origin,
        };
        let name = node
            .label()
            .or_else(|| node.value().filter(|_| node.role() == Role::Label))
            .unwrap_or_default()
            .to_owned();
        let actions: Vec<Action> = ACTIONS
            .iter()
            .copied()
            .filter(|a| node.supports_action(*a))
            .collect();
        let structural = matches!(
            node.role(),
            Role::GenericContainer | Role::Unknown | Role::Group | Role::ScrollView
        );
        let keep = id == root || !(structural && name.is_empty() && actions.is_empty());
        let mut next_parent = parent;
        if keep {
            let bounds = node.bounds().map(|r| {
                Geometry::new(
                    (r.x0.round() as i32, r.y0.round() as i32).into(),
                    (
                        (r.x1 - r.x0).round().max(0.0) as i32,
                        (r.y1 - r.y0).round().max(0.0) as i32,
                    )
                        .into(),
                )
            });
            let element = Element {
                id: id.0,
                parent,
                role: node.role(),
                value: node.value().filter(|v| *v != name).map(str::to_owned),
                name,
                description: node.description().map(str::to_owned),
                bounds,
                window,
                origin,
                focused: update.focus == id,
                disabled: node.is_disabled(),
                toggled: node.toggled().map(|t| t == Toggled::True),
                selected: node.is_selected(),
                expanded: node.is_expanded(),
                actions,
                children: Vec::new(),
            };
            if let Some(p) = parent.and_then(|p| index.get(&p).copied()) {
                let parent: &mut Element = &mut snapshot.elements[p];
                parent.children.push(id.0);
            }
            if element.focused {
                snapshot.focus = Some(id.0);
            }
            index.insert(id.0, snapshot.elements.len());
            snapshot.elements.push(element);
            next_parent = Some(id.0);
        }
        for child in node.children().iter().rev() {
            stack.push((*child, next_parent, window));
        }
    }
    snapshot
}

/// Actions worth telling agents about.
const ACTIONS: [Action; 10] = [
    Action::Click,
    Action::Focus,
    Action::SetValue,
    Action::Increment,
    Action::Decrement,
    Action::Expand,
    Action::Collapse,
    Action::ScrollIntoView,
    Action::ScrollUp,
    Action::ScrollDown,
];

#[cfg(test)]
mod tests {
    use super::*;

    fn egui_like(root: u64, nodes: &[(u64, Role, &str, &[u64])], focus: u64) -> TreeUpdate {
        TreeUpdate {
            nodes: nodes
                .iter()
                .map(|(id, role, label, children)| {
                    let mut n = Node::new(*role);
                    if !label.is_empty() {
                        n.set_label(*label);
                    }
                    if *role == Role::Button {
                        n.add_action(Action::Click);
                        n.add_action(Action::Focus);
                    }
                    n.set_children(children.iter().map(|c| NodeId(*c)).collect::<Vec<_>>());
                    (NodeId(*id), n)
                })
                .collect(),
            tree: Some(Tree::new(NodeId(root))),
            tree_id: TreeId::ROOT,
            focus: NodeId(focus),
        }
    }

    fn window<'a>(
        n: u64,
        title: &'static str,
        content: Option<&'a TreeUpdate>,
        focused: bool,
    ) -> WindowInfo<'a> {
        WindowInfo {
            window: WindowId::new(n).unwrap(),
            title,
            app_id: "org.example.app",
            frame: Geometry::new((10, 20).into(), (300, 200).into()),
            focused,
            content,
        }
    }

    #[test]
    fn same_egui_ids_in_two_contexts_get_distinct_stable_ids() {
        let chrome = egui_like(
            1,
            &[
                (1, Role::GenericContainer, "", &[7]),
                (7, Role::Button, "Overview", &[]),
            ],
            1,
        );
        let app = egui_like(
            1,
            &[
                (1, Role::GenericContainer, "", &[7]),
                (7, Role::Button, "Save", &[]),
            ],
            7,
        );
        let mut merger = Merger::new("desktop");
        let windows = [window(1, "Editor", Some(&app), true)];
        let first = merger.build(Some(&chrome), &windows, &[]);
        let snap = snapshot(&merger, &first);
        let names: Vec<&str> = snap.elements.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["desktop", "Overview", "Editor", "Save"]);
        let save = snap.elements.iter().find(|e| e.name == "Save").unwrap();
        let overview = snap.elements.iter().find(|e| e.name == "Overview").unwrap();
        assert_ne!(save.id, overview.id);
        assert_eq!(save.origin, Origin::App);
        assert_eq!(overview.origin, Origin::Chrome);
        assert_eq!(
            snap.focus,
            Some(save.id),
            "the focused app's focused widget"
        );

        let rebuilt = merger.build(Some(&chrome), &windows, &[]);
        let again = snapshot(&merger, &rebuilt);
        assert_eq!(again.get(save.id).map(|e| e.name.as_str()), Some("Save"));
        assert_eq!(
            merger.source(NodeId(save.id)),
            Some(Source::App(WindowId::new(1).unwrap(), NodeId(7)))
        );
    }

    #[test]
    fn unnamed_containers_are_flattened_and_windows_listed_topmost_first() {
        let chrome = egui_like(1, &[(1, Role::GenericContainer, "", &[])], 1);
        let mut merger = Merger::new("desktop");
        let windows = [
            window(1, "Bottom", None, false),
            window(2, "Top", None, false),
        ];
        let update = merger.build(Some(&chrome), &windows, &[]);
        let snap = snapshot(&merger, &update);
        let names: Vec<&str> = snap.elements.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["desktop", "Top", "Bottom"]);
        assert_eq!(
            snap.focus,
            Some(snap.elements[0].id),
            "nothing focused: the root"
        );
        assert_eq!(snap.elements[1].parent, Some(snap.elements[0].id));
    }

    #[test]
    fn shell_subtrees_hang_under_their_window() {
        let mut close = Node::new(Role::Button);
        close.set_label("Close");
        close.add_action(Action::Click);
        let w = WindowId::new(3).unwrap();
        let subtree = Subtree {
            window: w,
            roots: vec![NodeId(1)],
            nodes: vec![(NodeId(1), close)],
            origin: Origin::Shell,
        };
        let mut merger = Merger::new("desktop");
        let update = merger.build(None, &[window(3, "Term", None, true)], &[subtree]);
        let snap = snapshot(&merger, &update);
        let close = snap.elements.iter().find(|e| e.name == "Close").unwrap();
        assert_eq!(close.origin, Origin::Shell);
        assert_eq!(close.window, Some(w));
        assert_eq!(
            merger.source(NodeId(close.id)),
            Some(Source::Shell(w, NodeId(1), Origin::Shell))
        );
        let term = snap.get(close.parent.unwrap()).unwrap();
        assert_eq!(term.name, "Term");
        assert!(term.focused);
    }

    #[test]
    fn text_selections_follow_their_runs_or_go() {
        let mut field = Node::new(Role::TextInput);
        field.set_children(vec![NodeId(3)]);
        let at = |node| accesskit::TextPosition {
            node: NodeId(node),
            character_index: 1,
        };
        field.set_text_selection(accesskit::TextSelection {
            anchor: at(3),
            focus: at(3),
        });
        let mut broken = Node::new(Role::TextInput);
        broken.set_text_selection(accesskit::TextSelection {
            anchor: at(99),
            focus: at(99),
        });
        let mut root = Node::new(Role::GenericContainer);
        root.set_children(vec![NodeId(2), NodeId(4)]);
        let update = TreeUpdate {
            nodes: vec![
                (NodeId(1), root),
                (NodeId(2), field),
                (NodeId(3), Node::new(Role::TextRun)),
                (NodeId(4), broken),
            ],
            tree: Some(Tree::new(NodeId(1))),
            tree_id: TreeId::ROOT,
            focus: NodeId(1),
        };
        let mut merger = Merger::new("desktop");
        let merged = merger.build(Some(&update), &[], &[]);
        let id = |src| *merger.ids.get(&Source::Chrome(NodeId(src))).unwrap();
        let node = |n| &merged.nodes.iter().find(|(i, _)| *i == id(n)).unwrap().1;
        let selection = node(2).text_selection().unwrap();
        assert_eq!(selection.anchor.node, id(3));
        assert_eq!(selection.focus.character_index, 1);
        assert!(
            node(4).text_selection().is_none(),
            "dangling selection dropped"
        );
    }

    #[test]
    fn vanished_nodes_are_forgotten_and_cycles_do_not_hang() {
        let looped = egui_like(
            1,
            &[
                (1, Role::GenericContainer, "", &[2]),
                (2, Role::Button, "A", &[1, 2]),
            ],
            1,
        );
        let mut merger = Merger::new("desktop");
        let update = merger.build(Some(&looped), &[], &[]);
        assert_eq!(snapshot(&merger, &update).elements.len(), 2);
        let empty = egui_like(1, &[(1, Role::GenericContainer, "", &[])], 1);
        merger.build(Some(&empty), &[], &[]);
        assert_eq!(merger.ids.len(), 1, "only the root is left");
    }
}
