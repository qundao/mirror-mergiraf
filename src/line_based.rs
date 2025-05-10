use crate::{MergeResult, TSParser, parse, pcs::Revision};
use diffy_imara::{Algorithm, ConflictStyle, MergeOptions};
use typed_arena::Arena;

use crate::{lang_profile::LangProfile, parsed_merge::ParsedMerge, settings::DisplaySettings};
pub const LINE_BASED_METHOD: &str = "line_based";

pub fn line_based_merge_parsed(
    contents_base: &str,
    contents_left: &str,
    contents_right: &str,
    settings: &DisplaySettings,
) -> ParsedMerge<'static> {
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
        Ok(contents) | Err(contents) => contents.leak(),
    };
    ParsedMerge::parse(merged_contents, settings)
        .expect("diffy-imara returned a merge that we cannot parse the conflicts of")
}

/// Perform a textual merge with the diff3 algorithm.
pub fn line_based_merge(
    contents_base: &str,
    contents_left: &str,
    contents_right: &str,
    settings: &DisplaySettings,
) -> MergeResult {
    let parsed_merge =
        line_based_merge_parsed(contents_base, contents_left, contents_right, settings);

    parsed_merge.into_merge_result(settings)
}

/// Do a line-based merge. If it is conflict-free, also check if it introduced any duplicate signatures,
/// in which case this is logged as an additional issue on the merge result.
pub(crate) fn line_based_merge_with_duplicate_signature_detection(
    contents_base: &str,
    contents_left: &str,
    contents_right: &str,
    settings: &DisplaySettings,
    lang_profile: &LangProfile,
) -> (ParsedMerge<'static>, MergeResult) {
    let parsed_merge =
        line_based_merge_parsed(contents_base, contents_left, contents_right, settings);

    let mut merge_result = parsed_merge.into_merge_result(settings);

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

    merge_result.has_additional_issues = if merge_result.conflict_count == 0 {
        revision_has_issues(&merge_result.contents)
    } else {
        [Revision::Base, Revision::Left, Revision::Right]
            .into_iter()
            .map(|rev| parsed_merge.reconstruct_revision(rev))
            .any(|contents| revision_has_issues(&contents))
    };

    (parsed_merge, merge_result)
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

        let lang_profile = LangProfile::go();

        let (_, merge) = line_based_merge_with_duplicate_signature_detection(
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
