use crate::{parse, pcs::Revision, MergeResult, TSParser};
use diffy_imara::{Algorithm, ConflictStyle, MergeOptions};
use typed_arena::Arena;

use crate::{lang_profile::LangProfile, parsed_merge::ParsedMerge, settings::DisplaySettings};
pub const LINE_BASED_METHOD: &str = "line_based";
pub const STRUCTURED_RESOLUTION_METHOD: &str = "structured_resolution";
pub const FULLY_STRUCTURED_METHOD: &str = "fully_structured";

/// Perform a textual merge with the diff3 algorithm.
pub fn line_based_merge(
    contents_base: &str,
    contents_left: &str,
    contents_right: &str,
    settings: Option<&DisplaySettings>,
) -> MergeResult {
    let settings = if let Some(settings) = settings {
        settings
    } else {
        &DisplaySettings::default()
    };
    let merged = MergeOptions::new()
        .set_conflict_marker_length(settings.conflict_marker_size_or_default())
        .set_conflict_style(if settings.diff3 {
            ConflictStyle::Diff3
        } else {
            ConflictStyle::Merge
        })
        .set_algorithm(Algorithm::Histogram)
        .merge(contents_base, contents_left, contents_right);
    let merged_contents = match merged {
        Ok(contents) | Err(contents) => contents,
    };
    let parsed_merge = ParsedMerge::parse(&merged_contents, settings)
        .expect("diffy-imara returned a merge that we cannot parse the conflicts of");
    MergeResult {
        contents: parsed_merge.render(settings),
        conflict_count: parsed_merge.conflict_count(),
        conflict_mass: parsed_merge.conflict_mass(),
        method: LINE_BASED_METHOD,
        has_additional_issues: true,
    }
}

/// Do a line-based merge. If it is conflict-free, also check if it introduced any duplicate signatures,
/// in which case this is logged as an additional issue on the merge result.
pub(crate) fn line_based_merge_with_duplicate_signature_detection(
    contents_base: &str,
    contents_left: &str,
    contents_right: &str,
    settings: &DisplaySettings,
    lang_profile: &LangProfile,
) -> MergeResult {
    let mut line_based_merge =
        line_based_merge(contents_base, contents_left, contents_right, Some(settings));

    let mut parser = TSParser::new();
    parser
        .set_language(&lang_profile.language)
        .unwrap_or_else(|_| panic!("Error loading {} grammar", lang_profile.name));

    let mut revision_has_issues = |contents: &str| {
        let arena = Arena::new();
        let ref_arena = Arena::new();

        let tree = parse(&mut parser, contents, lang_profile, &arena, &ref_arena);

        tree.map_or(true, |ast| lang_profile.has_signature_conflicts(ast.root()))
    };

    let has_issues = if line_based_merge.conflict_count == 0 {
        revision_has_issues(&line_based_merge.contents)
    } else {
        let parsed_merge = ParsedMerge::parse(&line_based_merge.contents, settings)
            .expect("diffy-imara returned a merge that we cannot parse the conflicts of");

        [Revision::Base, Revision::Left, Revision::Right]
            .into_iter()
            .map(|rev| parsed_merge.reconstruct_revision(rev))
            .any(|contents| revision_has_issues(&contents))
    };

    line_based_merge.has_additional_issues = has_issues;

    line_based_merge
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn some_reconstructed_revisions_do_not_parse() {
        let contents_base = r#"import "github.com/go-redis/redis/v8"

func foo(){}"#;

        let contents_left = r#"import "github.com/redis/go-redis/v9"

func foo(){}"#;

        let contents_right = r#"import (
	"fmt"
	"net"
	"net/url"

	"github.com/redis/go-redis/v9"
)

// a comment to split hunks
func foo(){}"#;

        let contents_expected = r#"<<<<<<< LEFT
import "github.com/redis/go-redis/v9"
||||||| BASE
import "github.com/go-redis/redis/v8"
=======
import (
	"fmt"
	"net"
	"net/url"
>>>>>>> RIGHT

	"github.com/redis/go-redis/v9"
)

// a comment to split hunks
func foo(){}"#;

        let lang_profile =
            LangProfile::detect_from_filename("foo.go").expect("no `lang_profile` for Go");

        let merge = line_based_merge_with_duplicate_signature_detection(
            contents_base,
            contents_left,
            contents_right,
            &Default::default(),
            lang_profile,
        );

        assert_eq!(&merge.contents, contents_expected);

        assert!(
            merge.has_additional_issues,
            "left and base reconstructed revisions shouldn't parse"
        );
    }
}
