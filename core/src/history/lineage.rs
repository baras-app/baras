//! Which server instance a visit belongs to, judged from NPC ids.
//!
//! Shared by Raid Summaries (runs stitched across logs) and the live session
//! (encounter sections), so both draw the line in the same place.

use hashbrown::HashSet;

use baras_types::history::InstanceVisit;

/// What the visits of one instance (or its lockout) have shown so far.
#[derive(Debug, Clone, Default)]
pub struct InstanceLineage {
    /// 0 until a visit with an NPC is absorbed
    pub prefix: i64,
    pub max_ctr: i64,
    pub trash: HashSet<i64>,
    /// A boss died: the group is saved to this instance's lockout
    pub has_kill: bool,
}

impl InstanceLineage {
    /// Whether `v` is another visit to the same instance or lockout. Both
    /// sides must have seen an NPC; the caller decides what no evidence means.
    pub fn continues(&self, v: &InstanceVisit, same_character: bool) -> bool {
        if v.prefix != self.prefix {
            // An emptied instance is torn down. Coming back under the lockout
            // lands in a new process with the kills intact; without kills (or
            // on another character) a new process is a fresh instance.
            return self.has_kill && same_character;
        }
        // An NPC that already existed when the earlier visit ended
        if v.min_ctr <= self.max_ctr {
            return true;
        }
        // Everything is newer. A kill locks the group to the instance, so
        // only before the first one can this be a reset, which shows as the
        // opening trash dying a second time.
        self.has_kill || !v.trash.iter().any(|c| self.trash.contains(c))
    }

    pub fn absorb(&mut self, v: &InstanceVisit) {
        if v.prefix != 0 {
            self.max_ctr = if self.prefix == v.prefix {
                self.max_ctr.max(v.max_ctr)
            } else {
                v.max_ctr
            };
            self.prefix = v.prefix;
        }
        self.trash.extend(v.trash.iter().copied());
    }
}
