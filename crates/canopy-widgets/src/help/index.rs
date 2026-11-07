//! The tree of keys that help searches: the bindings of a focus, and of each
//! menu they lead to.

use std::collections::{BTreeMap, VecDeque};

use canopy::{
    Context, NodeId,
    error::Result,
    input::{AvailableBinding, BindingSnapshot, key::Key},
};

use super::binding_list::{MOUSE_SORT_GROUP, acts, key_sort_key};

/// Most entries an index holds. A tree this large only comes from many keys
/// that open the same menus, and its later chains add nothing to a search.
const ENTRY_LIMIT: usize = 4096;

/// Separator between the menus of a trail and the description after them.
pub(super) const TRAIL_SEPARATOR: &str = " › ";

/// The bindings of a focus, and the bindings of each menu they reach.
///
/// Help captures the tree before its own modal opens, because the modal
/// suspends menus and so hides what they offer.
#[derive(Clone, Debug)]
pub struct HelpTree {
    /// The bindings of the focus.
    pub(super) snapshot: BindingSnapshot,
    /// What each reachable menu offers, by menu name.
    pub(super) menus: BTreeMap<String, Vec<AvailableBinding<Key>>>,
}

impl HelpTree {
    /// Capture the bindings of `focus`, or of the current focus, and of every
    /// menu they reach.
    pub(crate) fn capture(context: &dyn Context, focus: Option<NodeId>) -> Result<Self> {
        let snapshot = context.available_bindings(focus)?;
        let focus = Some(snapshot.focus);
        let mut menus = BTreeMap::new();
        let mut waiting = snapshot
            .bindings
            .iter()
            .filter_map(|binding| binding.menu.clone())
            .collect::<VecDeque<_>>();
        while let Some(menu) = waiting.pop_front() {
            if menus.contains_key(&menu) {
                continue;
            }
            let bindings = context.menu_bindings(focus, &menu)?;
            waiting.extend(bindings.iter().filter_map(|binding| binding.menu.clone()));
            menus.insert(menu, bindings);
        }
        Ok(Self { snapshot, menus })
    }

    /// A tree of `snapshot` alone, with no menus.
    #[cfg(test)]
    pub(super) fn flat(snapshot: BindingSnapshot) -> Self {
        Self {
            snapshot,
            menus: BTreeMap::new(),
        }
    }

    /// Return the snapshot of the focus.
    pub(super) fn snapshot(&self) -> &BindingSnapshot {
        &self.snapshot
    }
}

/// One way to reach one action: a chain of keys, and the menus on the way.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct HelpEntry {
    /// The label of each key, from where help opened.
    pub(super) chain: Vec<String>,
    /// The descriptions of the menus the chain passes through.
    pub(super) trail: Vec<String>,
    /// What the last key does.
    pub(super) description: String,
    /// Whether the last key opens a menu.
    pub(super) opens_menu: bool,
}

impl HelpEntry {
    /// Return the text a search matches, which is also the text its row
    /// shows before any menu marker: the trail, then the description.
    pub(super) fn haystack(&self) -> String {
        let mut text = String::new();
        for menu in &self.trail {
            text.push_str(menu);
            text.push_str(TRAIL_SEPARATOR);
        }
        text.push_str(&self.description);
        text
    }

    /// Return the chain as one label, with a space between the keys.
    pub(super) fn chain_label(&self) -> String {
        self.chain.join(" ")
    }
}

/// Return every entry of `tree`, in tree order: each level by key, with the
/// entries of a menu right after the key that opens it.
///
/// Each distinct chain is one entry. A chain never passes through the same
/// menu twice, so a cycle of menus ends. Hidden bindings, and bindings whose
/// command cannot run now, are left out, as the list leaves them out.
pub(super) fn entries(tree: &HelpTree) -> Vec<HelpEntry> {
    let mut out = Vec::new();
    let mut walk = Walk {
        menus: &tree.menus,
        path: Vec::new(),
        out: &mut out,
    };
    walk.level(&tree.snapshot.bindings, &[], &[]);
    let mut mice = tree
        .snapshot
        .mouse_bindings
        .iter()
        .filter(|binding| binding.show_in_help && acts(binding))
        .collect::<Vec<_>>();
    mice.sort_by_key(|binding| (MOUSE_SORT_GROUP, binding.input.to_string()));
    for binding in mice {
        if out.len() >= ENTRY_LIMIT {
            break;
        }
        out.push(HelpEntry {
            chain: vec![binding.input.to_string()],
            trail: Vec::new(),
            description: binding.description.clone(),
            opens_menu: false,
        });
    }
    out
}

/// The state of one walk over the tree.
struct Walk<'a> {
    /// What each menu offers.
    menus: &'a BTreeMap<String, Vec<AvailableBinding<Key>>>,
    /// The menus the current chain passes through.
    path: Vec<&'a str>,
    /// Entries so far.
    out: &'a mut Vec<HelpEntry>,
}

impl<'a> Walk<'a> {
    /// Add the entries of one level, under `chain` and `trail`.
    fn level(&mut self, bindings: &'a [AvailableBinding<Key>], chain: &[String], trail: &[String]) {
        let mut shown = bindings
            .iter()
            .filter(|binding| binding.show_in_help && acts(binding))
            .collect::<Vec<_>>();
        shown.sort_by_key(|binding| key_sort_key(binding.input));
        for binding in shown {
            if self.out.len() >= ENTRY_LIMIT {
                return;
            }
            let mut next = chain.to_vec();
            next.push(Key::label(&binding.input));
            self.out.push(HelpEntry {
                chain: next.clone(),
                trail: trail.to_vec(),
                description: binding.description.clone(),
                opens_menu: binding.menu.is_some(),
            });
            let Some(menu) = binding.menu.as_deref() else {
                continue;
            };
            let Some(offered) = self.menus.get(menu) else {
                continue;
            };
            if self.path.contains(&menu) {
                continue;
            }
            let mut deeper = trail.to_vec();
            deeper.push(binding.description.clone());
            self.path.push(menu);
            self.level(offered, &next, &deeper);
            self.path.pop();
        }
    }
}
