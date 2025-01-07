use crate::attempts::Attempt;
use log::info;

/// A merged output (represented as a string) together with statistics
/// about the conflicts it contains.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MergeResult {
    /// The output of the merge (the file contents possibly with conflicts)
    pub contents: String,
    /// The number of conflicts
    pub conflict_count: usize,
    /// The sum of the sizes of conflicts
    pub conflict_mass: usize,
    /// A name for the merge, identifying with which technique it was produced
    pub method: &'static str,
    /// Indicates that there are known conflicts which haven't been marked as such (such as duplicate signatures)
    pub has_additional_issues: bool,
}

impl MergeResult {
    /// Helper to store a merge result in an attempt
    pub(crate) fn store_in_attempt(&self, attempt: &Attempt) {
        attempt.write(self.method, &self.contents).ok();
    }

    /// Helper to store a merge result in an attempt
    pub(crate) fn mark_as_best_merge_in_attempt(
        &self,
        attempt: &Attempt,
        line_based_conflicts: usize,
    ) {
        attempt.write_best_merge_id(self.method).ok();
        if self.conflict_count == 0 && line_based_conflicts > 0 {
            match line_based_conflicts {
                1 => {
                    info!(
                        "Mergiraf: Solved 1 conflict. Review with: mergiraf review {}",
                        attempt.id()
                    );
                }
                n => {
                    info!(
                        "Mergiraf: Solved {n} conflicts. Review with: mergiraf review {}",
                        attempt.id()
                    );
                }
            }
        }
    }
}
