use std::sync::LazyLock;

use crate::{
    lang_profile::{CommutativeParent, LangProfile},
    signature::{
        PathStep::{ChildType, Field},
        signature,
    },
};

/// The list of supported language profiles,
/// which contain all the language-specific information required to merge files in that language.
pub static SUPPORTED_LANGUAGES: LazyLock<Vec<LangProfile>> = LazyLock::new(|| {
    let typescript_commutative_parents = vec![
        CommutativeParent::without_delimiters("program", "\n")
            .restricted_to_groups(&[&["import_statement"]]),
        CommutativeParent::new("named_imports", "{", ", ", "}"),
        CommutativeParent::new("object", "{", ", ", "}"),
        CommutativeParent::new("class_body", " {\n", "\n\n", "\n}\n"),
        CommutativeParent::new("interface_body", " {\n", ";\n", "\n}\n"),
        CommutativeParent::new("object_type", " {\n", ";\n", "\n}\n"),
        CommutativeParent::new("enum_body", " {\n", ",\n", "\n}\n"),
        CommutativeParent::new("object_pattern", "{", ", ", "}"),
    ];
    let typescript_signatures = vec![
        signature("import_specifier", vec![vec![Field("name")]]),
        signature("pair", vec![vec![Field("key")]]),
        signature("identifier", vec![vec![]]),
        signature("method_definition", vec![vec![Field("name")]]),
        signature("public_field_definition", vec![vec![Field("name")]]),
        signature("property_signature", vec![vec![Field("name")]]),
        signature("property_identifier", vec![vec![]]),
        signature("pair_pattern", vec![vec![Field("key")]]),
    ];

    let tsx_commutative_parents = [
        typescript_commutative_parents.clone(),
        vec![CommutativeParent::new("jsx_opening_element", "<", " ", ">")],
    ]
    .concat();
    let tsx_signatures = [
        typescript_signatures.clone(),
        vec![signature(
            "jsx_attribute",
            vec![vec![ChildType("identifier")]],
        )],
    ]
    .concat();

    vec![
        LangProfile {
            name: "Java",
            extensions: vec!["java"],
            language: tree_sitter_java::LANGUAGE.into(),
            atomic_nodes: vec!["import_declaration"],
            commutative_parents: vec![
                // top-level node, for imports and class declarations
                CommutativeParent::without_delimiters("program", "\n").restricted_to_groups(&[
                    &["module_declaration"],
                    &["package_declaration"],
                    &["import_declaration"],
                    &[
                        "class_declaration",
                        "record_declaration",
                        "interface_declaration",
                        "annotation_type_declaration",
                        "enum_declaration",
                    ],
                ]),
                // strictly speaking, this isn't true (order can be accessed via reflection)
                CommutativeParent::new("class_body", " {\n", "\n", "\n}\n").restricted_to_groups(
                    &[
                        &["field_declaration"],
                        &[
                            "record_declaration",
                            "class_declaration",
                            "interface_declaration",
                            "annotation_type_declaration",
                            "enum_declaration",
                        ],
                        &[
                            "constructor_declaration",
                            "method_declaration",
                            "compact_constructor_declaration",
                        ],
                    ],
                ),
                CommutativeParent::new("interface_body", " {\n", "\n", "\n}\n")
                    .restricted_to_groups(&[
                        &["field_declaration"],
                        &[
                            "record_declaration",
                            "class_declaration",
                            "interface_declaration",
                            "annotation_type_declaration",
                            "enum_declaration",
                        ],
                        &["method_declaration"],
                    ]),
                CommutativeParent::without_delimiters("modifiers", " ").restricted_to_groups(&[
                    &[
                        "public",
                        "protected",
                        "private",
                        "abstract",
                        "static",
                        "final",
                        "strictfp",
                        "default",
                        "synchronized",
                        "native",
                        "transient",
                        "volatile",
                        "sealed",
                        "non-sealed",
                    ],
                    &["marker_annotation", "annotation"],
                ]),
                CommutativeParent::without_delimiters("catch_type", " | "),
                CommutativeParent::without_delimiters("type_list", ", "), // for "implements" or "sealed"
                CommutativeParent::new("annotation_argument_list", "{", ", ", "}"),
            ],
            signatures: vec![
                // program
                signature("import_declaration", vec![vec![]]),
                signature("class_declaration", vec![vec![Field("name")]]),
                // class_body
                signature(
                    "field_declaration",
                    vec![vec![Field("declarator"), Field("name")]],
                ),
                signature(
                    "method_declaration",
                    vec![
                        vec![Field("name")],
                        vec![
                            Field("parameters"),
                            ChildType("formal_parameter"),
                            Field("type"),
                        ],
                        vec![
                            Field("parameters"),
                            ChildType("spread_parameter"),
                            ChildType("identifier"),
                        ],
                    ],
                ),
                // modifiers
                signature("public", vec![vec![]]),
                signature("protected", vec![vec![]]),
                signature("private", vec![vec![]]),
                signature("static", vec![vec![]]),
                signature("final", vec![vec![]]),
                signature("sealed", vec![vec![]]),
                // catch_type & type_list
                signature("identifier", vec![vec![]]),
                // annotation_argument_list
                signature("element_value_pair", vec![vec![Field("key")]]),
            ],
        },
        LangProfile {
            name: "Java properties",
            extensions: vec!["properties"],
            language: tree_sitter_properties::LANGUAGE.into(),
            atomic_nodes: vec![],
            commutative_parents: vec![CommutativeParent::without_delimiters("file", "\n")],
            signatures: vec![signature("property", vec![vec![ChildType("key")]])],
        },
        LangProfile {
            name: "Kotlin",
            extensions: vec!["kt"],
            language: tree_sitter_kotlin_ng::LANGUAGE.into(),
            atomic_nodes: vec![],
            commutative_parents: vec![
                // top-level node, for imports and class declarations
                CommutativeParent::without_delimiters("source_file", "\n\n")
                    .restricted_to_groups(&[&["import"], &["function_declaration"]]),
                CommutativeParent::new("class_body", " {\n", "\n\n", "\n}\n")
                    .restricted_to_groups(&[&["property_declaration"], &["function_declaration"]]),
                CommutativeParent::without_delimiters("modifiers", "\n").restricted_to_groups(&[
                    &["annotation"],
                    &[
                        "visibility_modifier",
                        "inheritance_modifier",
                        "member_modifier",
                    ],
                ]),
                CommutativeParent::without_delimiters("class_declaration", ", ")
                    .restricted_to_groups(&[&["delegation_specifier"]]),
            ],
            signatures: vec![
                signature("import", vec![vec![]]),
                signature(
                    "function_declaration",
                    vec![
                        vec![Field("name")],
                        vec![
                            ChildType("function_value_parameters"),
                            ChildType("parameter"),
                            ChildType("user_type"),
                        ],
                    ],
                ),
                signature("delegation_specifier", vec![vec![]]),
                signature("public", vec![vec![]]),
                signature("protected", vec![vec![]]),
                signature("private", vec![vec![]]),
                signature("internal", vec![vec![]]),
                signature("final", vec![vec![]]),
                signature("open", vec![vec![]]),
                signature("abstract", vec![vec![]]),
                signature("override", vec![vec![]]),
            ],
        },
        LangProfile {
            name: "Rust",
            extensions: vec!["rs"],
            language: tree_sitter_rust::LANGUAGE.into(),
            atomic_nodes: vec![],
            commutative_parents: vec![
                CommutativeParent::without_delimiters("source_file", "\n").restricted_to_groups(&[
                    &["use_declaration"], // to keep use declarations together (even if it's not actually required)
                    &[
                        "const_item",
                        "macro_definition",
                        "mod_item",
                        "foreign_mod_item",
                        "struct_item",
                        "union_item",
                        "enum_item",
                        "type_item",
                        "function_item",
                        "function_signature_item",
                        "trait_item",
                        "associated_type",
                        "let_declaration",
                        "extern_crate_declaration",
                        "static_item",
                    ],
                ]),
                // module members, impls…
                CommutativeParent::new("declaration_list", " {\n", "\n", "\n}\n")
                    .restricted_to_groups(&[
                        &["use_declaration"], // to keep use declarations together (even if it's not actually required)
                        &[
                            "const_item",
                            "macro_definition",
                            "mod_item",
                            "foreign_mod_item",
                            "struct_item",
                            "union_item",
                            "enum_item",
                            "type_item",
                            "function_item",
                            "function_signature_item",
                            "trait_item",
                            "associated_type",
                            "let_declaration",
                            "extern_crate_declaration",
                            "static_item",
                        ],
                    ]),
                // scoped "use" declaration
                CommutativeParent::new("use_list", "{", ", ", "}"),
                CommutativeParent::with_left_delimiter("trait_bounds", ": ", " + "),
                // strictly speaking, the derived order on values depends on their declaration
                CommutativeParent::new("enum_variant_list", " {\n", ", ", "\n}\n")
                    .restricted_to_groups(&[&["enum_variant"]]),
                // strictly speaking, the order can matter if using the C ABI
                CommutativeParent::new("field_declaration_list", " {\n", ", ", "\n}\n")
                    .restricted_to_groups(&[&["field_declaration"]]),
                CommutativeParent::new("field_initializer_list", "{ ", ", ", " }")
                    .restricted_to_groups(&[&[
                        "shorthand_field_initializer",
                        "field_initializer",
                        "base_field_initializer",
                    ]]),
                CommutativeParent::without_delimiters("function_modifiers", " "),
                CommutativeParent::with_left_delimiter("where_clause", "where", ",\n")
                    .restricted_to_groups(&[&["where_predicate"]]),
            ],
            signatures: vec![
                // as module member, impls…
                signature("const_item", vec![vec![Field("name")]]),
                signature("macro_definition", vec![vec![Field("name")]]),
                signature("mod_item", vec![vec![Field("name")]]),
                signature("struct_item", vec![vec![Field("name")]]),
                signature("union_item", vec![vec![Field("name")]]),
                signature("enum_item", vec![vec![Field("name")]]),
                signature("type_item", vec![vec![Field("name")]]),
                signature("function_item", vec![vec![Field("name")]]),
                signature("function_signature_item", vec![vec![Field("name")]]),
                signature("trait_item", vec![vec![Field("name")]]),
                signature("static_item", vec![vec![Field("name")]]),
                signature("associated_type", vec![vec![Field("name")]]),
                // function_modifiers
                signature("async", vec![vec![]]),
                signature("default", vec![vec![]]),
                signature("const", vec![vec![]]),
                signature("unsafe", vec![vec![]]),
                // source_file
                signature("use_declaration", vec![vec![Field("argument")]]),
                // trait_bound
                signature("lifetime", vec![vec![]]),
                signature("identifier", vec![vec![]]),
                // enum_variant_list
                signature("enum_variant", vec![vec![Field("name")]]),
                // field_declaration_list
                signature("field_declaration", vec![vec![Field("name")]]),
                // field_initializer_list
                signature("field_initializer", vec![vec![Field("field")]]),
                signature("shorthand_field_initializer", vec![vec![]]),
                signature("base_field_initializer", vec![]), // maximum one per field_initializer_list
            ],
        },
        LangProfile {
            name: "Go",
            extensions: vec!["go"],
            language: tree_sitter_go::LANGUAGE.into(),
            atomic_nodes: vec!["interpreted_string_literal"], // for https://github.com/tree-sitter/tree-sitter-go/issues/150
            commutative_parents: vec![
                CommutativeParent::without_delimiters("source_file", "\n").restricted_to_groups(&[
                    &["import_declaration"],
                    &["function_declaration", "method_declaration"],
                ]),
                CommutativeParent::new("import_spec_list", "(\n", "\n", "\n)\n")
                    .restricted_to_groups(&[&["import_spec"]]),
                CommutativeParent::new("field_declaration_list", " {\n", "\n", "\n}\n") // not strictly speaking, because it impacts memory layout
                    .restricted_to_groups(&[&["field_declaration"]]),
                CommutativeParent::new("literal_value", "{", ", ", "}")
                    .restricted_to_groups(&[&["literal_element", "keyed_element"]]),
            ],
            signatures: vec![
                signature(
                    "type_declaration",
                    vec![vec![ChildType("type_spec"), Field("name")]],
                ),
                signature("field_declaration", vec![vec![Field("name")]]),
                signature("function_declaration", vec![vec![Field("name")]]),
                signature(
                    "method_declaration",
                    vec![vec![Field("receiver")], vec![Field("name")]],
                ),
                signature("import_spec", vec![vec![Field("path")]]), // ideally we'd also take the 'name' into account, as it must probably be unique
                signature("keyed_element", vec![vec![Field("key")]]),
            ],
        },
        LangProfile {
            name: "Javascript",
            extensions: vec!["js", "jsx", "mjs"],
            language: tree_sitter_javascript::LANGUAGE.into(),
            atomic_nodes: vec![],
            commutative_parents: vec![
                CommutativeParent::new("object", "{", ", ", "}"),
                CommutativeParent::new("class_body", " {\n", "\n\n", "\n}\n"),
                CommutativeParent::new("jsx_opening_element", "<", " ", ">"),
            ],
            signatures: vec![
                signature("pair", vec![vec![Field("key")]]),
                signature("identifier", vec![vec![]]),
                signature("method_definition", vec![vec![Field("name")]]),
                signature("jsx_attribute", vec![vec![ChildType("identifier")]]),
            ],
        },
        LangProfile {
            name: "JSON",
            extensions: vec!["json"],
            language: tree_sitter_json::LANGUAGE.into(),
            atomic_nodes: vec![],
            commutative_parents: vec![
                // the order of keys is deemed irrelevant
                CommutativeParent::new("object", "{", ", ", "}"),
            ],
            signatures: vec![signature("pair", vec![vec![Field("key")]])],
        },
        LangProfile {
            name: "YAML",
            extensions: vec!["yml", "yaml"],
            language: tree_sitter_yaml::LANGUAGE.into(),
            atomic_nodes: vec![],
            commutative_parents: vec![CommutativeParent::without_delimiters("block_mapping", "\n")],
            signatures: vec![signature("block_mapping_pair", vec![vec![Field("key")]])],
        },
        LangProfile {
            name: "TOML",
            extensions: vec!["toml"],
            language: tree_sitter_toml_ng::LANGUAGE.into(),
            atomic_nodes: vec!["string"],
            commutative_parents: vec![
                CommutativeParent::without_delimiters("document", "\n"),
                CommutativeParent::without_delimiters("table", "\n"),
                CommutativeParent::new("inline_table", "{", ", ", "}"),
            ],
            signatures: vec![
                signature("pair", vec![vec![ChildType("bare_key")]]),
                signature("_inline_pair", vec![vec![ChildType("bare_key")]]),
            ],
        },
        LangProfile {
            name: "HTML",
            extensions: vec!["html", "htm"],
            language: tree_sitter_html::LANGUAGE.into(),
            atomic_nodes: vec![],
            commutative_parents: vec![
                CommutativeParent::new("self_closing_tag", "<", " ", "/>"),
                CommutativeParent::new("start_tag", "<", " ", ">"),
            ],
            signatures: vec![signature(
                "attribute",
                vec![vec![ChildType("attribute_name")]],
            )],
        },
        LangProfile {
            name: "XML",
            extensions: vec!["xhtml", "xml"],
            language: tree_sitter_xml::LANGUAGE_XML.into(),
            atomic_nodes: vec!["AttValue"],
            commutative_parents: vec![
                CommutativeParent::new("EmptyElemTag", "<", " ", "/>"),
                CommutativeParent::new("STag", "<", " ", ">"),
            ],
            signatures: vec![signature("Attribute", vec![vec![ChildType("Name")]])],
        },
        LangProfile {
            name: "C/C++",
            extensions: vec!["c", "h", "cc", "cpp", "hpp"],
            language: tree_sitter_cpp::LANGUAGE.into(),
            atomic_nodes: vec![],
            commutative_parents: vec![
                CommutativeParent::new("initializer_list", "{", ",", "}"),
                CommutativeParent::new("field_declaration_list", "{\n", "\n", "\n}\n")
                    .restricted_to_groups(&[&["field_declaration"], &["function_definition"]]),
            ],
            signatures: vec![
                signature("initializer_pair", vec![vec![Field("designator")]]),
                signature(
                    "function_definition",
                    vec![
                        vec![Field("declarator"), Field("declarator")],
                        vec![
                            Field("declarator"),
                            Field("parameters"),
                            ChildType("parameter_declaration"),
                            Field("type"),
                        ],
                    ],
                ),
                signature("field_declaration", vec![vec![Field("declarator")]]), // TODO this isn't quite right, as the "*" of a pointer type will end up in the signature
            ],
        },
        LangProfile {
            name: "C#",
            extensions: vec!["cs"],
            language: tree_sitter_c_sharp::LANGUAGE.into(),
            atomic_nodes: vec![],
            commutative_parents: vec![
                CommutativeParent::without_delimiters("compilation_unit", "\n"),
                CommutativeParent::new("declaration_list", "{", "\n", "}"),
                CommutativeParent::new("enum_member_declaration_list", "{", ",\n", "}"),
            ],
            signatures: vec![
                signature("using_directive", vec![vec![]]),
                // declaration_list
                signature("class_declaration", vec![vec![Field("name")]]),
                signature("struct_declaration", vec![vec![Field("name")]]),
                signature("enum_declaration", vec![vec![Field("name")]]),
                signature("interface_declaration", vec![vec![Field("name")]]),
                signature("delegate_declaration", vec![vec![Field("name")]]),
                signature("record_declaration", vec![vec![Field("name")]]),
                signature("field_declaration", vec![vec![Field("name")]]),
                signature(
                    "method_declaration",
                    vec![
                        vec![Field("name")],
                        vec![Field("parameters"), ChildType("parameter"), Field("type")],
                    ],
                ),
                signature(
                    "constructor_declaration",
                    vec![
                        vec![Field("name")],
                        vec![Field("parameters"), ChildType("parameter"), Field("type")],
                    ],
                ),
                signature("destructor_declaration", vec![]), // only one destructor per class
                signature(
                    "operator_declaration",
                    vec![
                        vec![Field("operator")],
                        vec![Field("parameters"), ChildType("parameter"), Field("type")],
                    ],
                ),
                signature("event_declaration", vec![vec![Field("name")]]),
                // enum_declaration_list
                signature("enum_member_declaration", vec![vec![Field("name")]]),
            ],
        },
        LangProfile {
            name: "Dart",
            extensions: vec!["dart"],
            language: tree_sitter_dart::language(),
            atomic_nodes: vec!["import_or_export"],
            commutative_parents: vec![
                CommutativeParent::without_delimiters("program", "\n"),
                CommutativeParent::new("enum_body", "{", ",\n", "}"),
                CommutativeParent::new("class_body", "{", "\n", "}"),
            ],
            signatures: vec![
                signature("import_or_export", vec![vec![]]),
                signature("enum_constant", vec![vec![]]),
                signature("class_definition", vec![vec![Field("name")]]),
            ],
        },
        LangProfile {
            name: "Devicetree Source",
            extensions: vec!["dts"],
            language: tree_sitter_devicetree::LANGUAGE.into(),
            atomic_nodes: vec!["string_literal"],
            commutative_parents: vec![CommutativeParent::new("node", "{", "\n", "}")],
            signatures: vec![
                signature("property", vec![vec![Field("name")]]),
                signature("node", vec![vec![Field("name")]]),
            ],
        },
        LangProfile {
            name: "Scala",
            extensions: vec!["scala", "sbt"],
            language: tree_sitter_scala::LANGUAGE.into(),
            atomic_nodes: vec![],
            commutative_parents: vec![],
            signatures: vec![],
        },
        LangProfile {
            name: "Typescript",
            extensions: vec!["ts"],
            language: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            atomic_nodes: vec![],
            commutative_parents: typescript_commutative_parents,
            signatures: typescript_signatures,
        },
        LangProfile {
            name: "Typescript (TSX)",
            extensions: vec!["tsx"],
            language: tree_sitter_typescript::LANGUAGE_TSX.into(),
            atomic_nodes: vec![],
            commutative_parents: tsx_commutative_parents,
            signatures: tsx_signatures,
        },
        LangProfile {
            name: "Python",
            extensions: vec!["py"],
            language: tree_sitter_python::LANGUAGE.into(),
            atomic_nodes: vec!["string", "dotted_name"],
            commutative_parents: vec![
                CommutativeParent::without_delimiters("module", "\n").restricted_to_groups(&[
                    &["import_statement", "import_from_statement"],
                    &["class_definition"],
                ]),
                CommutativeParent::without_delimiters("block", "\n\n")
                    .restricted_to_groups(&[&["function_definition"]]),
                CommutativeParent::without_delimiters("import_from_statement", ", ")
                    .restricted_to_groups(&[&["dotted_name"]]),
                CommutativeParent::new("argument_list", "(", ", ", ")")
                    .restricted_to_groups(&[&["keyword_argument"]]),
                CommutativeParent::new("set", "{", ", ", "}"),
            ],
            signatures: vec![
                signature("import_from_statement", vec![vec![]]),
                signature("class_definition", vec![vec![Field("name")]]),
                signature("function_definition", vec![vec![Field("name")]]),
                signature("dotted_name", vec![vec![]]),
                signature("keyword_argument", vec![vec![Field("name")]]),
            ],
        },
        LangProfile {
            name: "PHP",
            extensions: vec!["php", "phtml"],
            language: tree_sitter_php::LANGUAGE_PHP.into(),
            // optional settings, explained below
            atomic_nodes: vec![],
            commutative_parents: vec![
                // TODO: allow commutation between "use" and "require" statements, which is
                // currently not possible as "require" statements appear as "expression_statement",
                // which encompasses non-declarative statements too.
                CommutativeParent::without_delimiters("program", "\n")
                    .restricted_to_groups(&[&["namespace_use_declaration"]]),
                CommutativeParent::new("declaration_list", "{", "\n\n", "}"),
                CommutativeParent::new("enum_declaration_list", "{", "\n\n", "}"),
            ],
            signatures: vec![
                signature("namespace_use_declaration", vec![vec![]]),
                signature(
                    "const_declaration",
                    vec![vec![ChildType("const_element"), ChildType("name")]],
                ),
                signature("function_definition", vec![vec![Field("name")]]),
                signature("interface_declaration", vec![vec![Field("name")]]),
                signature("class_declaration", vec![vec![Field("name")]]),
                signature(
                    "property_declaration",
                    vec![vec![ChildType("property_element"), Field("name")]],
                ),
                signature("property_promotion_parameter", vec![vec![Field("name")]]),
                signature("method_declaration", vec![vec![Field("name")]]),
                signature("enum_declaration", vec![vec![Field("name")]]),
                signature("enum_case", vec![vec![Field("name")]]),
                signature("attribute_list", vec![vec![]]),
            ],
        },
        LangProfile {
            name: "Solidity",
            extensions: vec!["sol"],
            language: tree_sitter_solidity::LANGUAGE.into(),
            atomic_nodes: vec![],
            commutative_parents: vec![
                CommutativeParent::without_delimiters("source_file", "\n"),
                CommutativeParent::without_delimiters("contract_body", "\n"),
            ],
            signatures: vec![],
        },
        LangProfile {
            name: "Lua",
            extensions: vec!["lua"],
            language: tree_sitter_lua::LANGUAGE.into(),
            atomic_nodes: vec![],
            commutative_parents: vec![],
            signatures: vec![],
        },
        LangProfile {
            name: "Ruby",
            extensions: vec!["rb"],
            language: tree_sitter_ruby::LANGUAGE.into(),
            atomic_nodes: vec![],
            commutative_parents: vec![],
            signatures: vec![],
        },
        LangProfile {
            name: "Nix",
            extensions: vec!["nix"],
            language: tree_sitter_nix::LANGUAGE.into(),
            atomic_nodes: vec![],
            commutative_parents: vec![
                CommutativeParent::new("binding_set", "{", "\n", "}"),
                CommutativeParent::new("formals", "{", ",\n", "}"),
            ],
            signatures: vec![
                signature("binding", vec![vec![Field("attrpath")]]),
                signature("formal", vec![vec![Field("name")]]),
            ],
        },
        LangProfile {
            name: "SystemVerilog",
            extensions: vec!["sv", "svh"],
            language: tree_sitter_verilog::LANGUAGE.into(),
            atomic_nodes: vec![],
            commutative_parents: vec![],
            signatures: vec![],
        },
    ]
});

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn extensions_do_not_start_with_a_dot() {
        for lang_profile in &*SUPPORTED_LANGUAGES {
            for ext in &lang_profile.extensions {
                assert!(!ext.starts_with('.'), "{ext}");
            }
        }
    }
}
