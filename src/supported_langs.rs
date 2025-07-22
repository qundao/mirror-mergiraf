use std::sync::LazyLock;

use crate::{
    lang_profile::{ChildrenGroup, CommutativeParent, LangProfile},
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

    let ocaml_commutative_parents = vec![
        /* Record fields */
        CommutativeParent::new("record_expression", "{", "; ", "}")
            .restricted_to_groups(&[&["field_expression"]]),
    ];
    let ocaml_signatures = vec![signature(
        "field_expression",
        vec![vec![
            ChildType("field_path"),
            ChildType("_lowercase_identifier"),
        ]],
    )];

    vec![
        LangProfile {
            name: "Java",
            alternate_names: &[],
            extensions: vec!["java"],
            file_names: vec![],
            language: tree_sitter_java::LANGUAGE.into(),
            atomic_nodes: vec!["import_declaration"],
            commutative_parents: vec![
                // top-level node, for imports and class declarations
                CommutativeParent::without_delimiters("program", "\n\n").restricted_to(vec![
                    ChildrenGroup::new(&["module_declaration"]),
                    ChildrenGroup::new(&["package_declaration"]),
                    ChildrenGroup::with_separator(&["import_declaration"], "\n"),
                    ChildrenGroup::new(&[
                        "class_declaration",
                        "record_declaration",
                        "interface_declaration",
                        "annotation_type_declaration",
                        "enum_declaration",
                    ]),
                ]),
                // strictly speaking, this isn't true (order can be accessed via reflection)
                CommutativeParent::new("class_body", " {\n", "\n\n", "\n}\n").restricted_to(vec![
                    ChildrenGroup::with_separator(&["field_declaration"], "\n"),
                    ChildrenGroup::new(&[
                        "record_declaration",
                        "class_declaration",
                        "interface_declaration",
                        "annotation_type_declaration",
                        "enum_declaration",
                    ]),
                    ChildrenGroup::new(&[
                        "constructor_declaration",
                        "method_declaration",
                        "compact_constructor_declaration",
                    ]),
                ]),
                CommutativeParent::new("interface_body", " {\n", "\n\n", "\n}\n").restricted_to(
                    vec![
                        ChildrenGroup::with_separator(&["field_declaration"], "\n"),
                        ChildrenGroup::new(&[
                            "record_declaration",
                            "class_declaration",
                            "interface_declaration",
                            "annotation_type_declaration",
                            "enum_declaration",
                        ]),
                        ChildrenGroup::new(&["method_declaration"]),
                    ],
                ),
                CommutativeParent::without_delimiters("modifiers", " ").restricted_to(vec![
                    ChildrenGroup::with_separator(
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
                        " ",
                    ),
                    ChildrenGroup::with_separator(&["marker_annotation", "annotation"], "\n"),
                ]),
                CommutativeParent::without_delimiters("throws", ", ")
                    .restricted_to_groups(&[&["identifier"]]),
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
                // catch_type, type_list, throws
                signature("identifier", vec![vec![]]),
                // annotation_argument_list
                signature("element_value_pair", vec![vec![Field("key")]]),
            ],
            injections: None,
        },
        LangProfile {
            name: "Java properties",
            alternate_names: &[],
            extensions: vec!["properties"],
            file_names: vec![],
            language: tree_sitter_properties::LANGUAGE.into(),
            atomic_nodes: vec![],
            commutative_parents: vec![CommutativeParent::without_delimiters("file", "\n")],
            signatures: vec![signature("property", vec![vec![ChildType("key")]])],
            injections: None,
        },
        LangProfile {
            name: "Kotlin",
            alternate_names: &[],
            extensions: vec!["kt"],
            file_names: vec![],
            language: tree_sitter_kotlin_ng::LANGUAGE.into(),
            atomic_nodes: vec![],
            commutative_parents: vec![
                // top-level node, for imports and class declarations
                CommutativeParent::without_delimiters("source_file", "\n\n")
                    .restricted_to_groups(&[&["import"], &["function_declaration"]]),
                CommutativeParent::new("class_body", " {\n", "\n\n", "\n}\n")
                    .restricted_to_groups(&[&["property_declaration"], &["function_declaration"]]),
                CommutativeParent::without_delimiters("modifiers", "\n").restricted_to(vec![
                    ChildrenGroup::new(&["annotation"]),
                    ChildrenGroup::with_separator(
                        &[
                            "visibility_modifier",
                            "inheritance_modifier",
                            "member_modifier",
                        ],
                        " ",
                    ),
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
            injections: None,
        },
        LangProfile {
            name: "Rust",
            alternate_names: &[],
            extensions: vec!["rs"],
            file_names: vec![],
            language: tree_sitter_rust_orchard::LANGUAGE.into(),
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
                            "macro_definition_v2",
                            "mod_item",
                            "foreign_mod_item",
                            "struct_item",
                            "union_item",
                            "enum_item",
                            "type_item",
                            "function_item",
                            "function_signature_item",
                            "trait_item",
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
                signature("macro_definition_v2", vec![vec![Field("name")]]),
                signature("mod_item", vec![vec![Field("name")]]),
                signature("struct_item", vec![vec![Field("name")]]),
                signature("union_item", vec![vec![Field("name")]]),
                signature("enum_item", vec![vec![Field("name")]]),
                signature("type_item", vec![vec![Field("name")]]),
                signature("function_item", vec![vec![Field("name")]]),
                signature("function_signature_item", vec![vec![Field("name")]]),
                signature("trait_item", vec![vec![Field("name")]]),
                signature("static_item", vec![vec![Field("name")]]),
                // function_modifiers
                signature("async", vec![vec![]]),
                signature("default", vec![vec![]]),
                signature("const", vec![vec![]]),
                signature("unsafe", vec![vec![]]),
                // source_file
                signature("use_declaration", vec![vec![Field("argument")]]),
                // trait_bound
                signature("lifetime", vec![vec![]]),
                // use list
                signature("self", vec![vec![]]),
                signature("identifier", vec![vec![]]),
                signature("scoped_identifier", vec![vec![]]),
                // enum_variant_list
                signature("enum_variant", vec![vec![Field("name")]]),
                // field_declaration_list
                signature("field_declaration", vec![vec![Field("name")]]),
                // field_initializer_list
                signature("field_initializer", vec![vec![Field("field")]]),
                signature("shorthand_field_initializer", vec![vec![]]),
                signature("base_field_initializer", vec![]), // maximum one per field_initializer_list
            ],
            injections: Some(tree_sitter_rust_orchard::INJECTIONS_QUERY),
        },
        LangProfile {
            name: "Go",
            alternate_names: &[],
            extensions: vec!["go"],
            file_names: vec![],
            language: tree_sitter_go::LANGUAGE.into(),
            atomic_nodes: vec!["interpreted_string_literal"], // for https://github.com/tree-sitter/tree-sitter-go/issues/150
            commutative_parents: vec![
                CommutativeParent::without_delimiters("source_file", "\n").restricted_to(vec![
                    ChildrenGroup::new(&["import_declaration"]),
                    ChildrenGroup::with_separator(
                        &["function_declaration", "method_declaration"],
                        "\n\n",
                    ),
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
            injections: None,
        },
        LangProfile {
            name: "go.mod",
            alternate_names: &[],
            extensions: vec![],
            file_names: vec!["go.mod"],
            language: tree_sitter_gomod_orchard::LANGUAGE.into(),
            atomic_nodes: vec![],
            commutative_parents: vec![
                CommutativeParent::without_delimiters("source_file", "\n\n").restricted_to_groups(
                    &[
                        &["require_directive_single"],
                        &["replace_directive_single"],
                        &["exclude_directive_single"],
                        &["retract_directive_single"],
                        &["ignore_directive_single"],
                        &["godebug_directive_single"],
                    ],
                ),
                CommutativeParent::new("require_directive_multi", "(", "\n", ")")
                    .restricted_to_groups(&[&["require_spec"]]),
                CommutativeParent::new("replace_directive_multi", "(", "\n", ")")
                    .restricted_to_groups(&[&["replace_spec"]]),
                CommutativeParent::new("exclude_directive_multi", "(", "\n", ")")
                    .restricted_to_groups(&[&["exclude_spec"]]),
                CommutativeParent::new("retract_directive_multi", "(", "\n", ")")
                    .restricted_to_groups(&[&["retract_spec"]]),
                CommutativeParent::new("ignore_directive_multi", "(", "\n", ")")
                    .restricted_to_groups(&[&["ignore_spec"]]),
                CommutativeParent::new("godebug_directive_multi", "(", "\n", ")")
                    .restricted_to_groups(&[&["godebug_spec"]]),
            ],
            signatures: vec![
                signature(
                    "require_directive_single",
                    vec![vec![ChildType("require_spec"), Field("path")]],
                ),
                signature(
                    "replace_directive_single",
                    vec![
                        vec![ChildType("replace_spec"), Field("from_path")],
                        vec![ChildType("replace_spec"), Field("from_version")],
                    ],
                ),
                signature(
                    "exclude_directive_single",
                    vec![vec![ChildType("exclude_spec")]],
                ),
                signature(
                    "retract_directive_single",
                    vec![vec![ChildType("retract_spec")]],
                ),
                signature(
                    "ignore_directive_single",
                    vec![vec![ChildType("ignore_spec")]],
                ),
                signature(
                    "godebug_directive_single",
                    vec![vec![ChildType("godebug_spec"), Field("key")]],
                ),
                signature("require_spec", vec![vec![Field("path")]]),
                signature(
                    "replace_spec",
                    vec![vec![Field("from_path")], vec![Field("from_version")]],
                ),
                signature("exclude_spec", vec![vec![]]),
                signature("retract_spec", vec![vec![]]),
                signature("ignore_spec", vec![vec![]]),
                signature("godebug_spec", vec![vec![Field("key")]]),
            ],
            injections: None,
        },
        LangProfile {
            name: "go.sum",
            alternate_names: &[],
            extensions: vec![],
            file_names: vec!["go.sum"],
            language: tree_sitter_gosum_orchard::LANGUAGE.into(),
            atomic_nodes: vec![],
            commutative_parents: vec![
                CommutativeParent::without_delimiters("checksum_database", "\n")
                    .restricted_to_groups(&[&["checksum"]]),
            ],
            signatures: vec![
                // the same module can appear multiple times in go.sum with the same version,
                // so the version needs to be part of the signature.
                signature(
                    "checksum",
                    vec![
                        vec![Field("path")],
                        vec![Field("version")],
                        vec![Field("go_mod")],
                    ],
                ),
            ],
            injections: None,
        },
        LangProfile {
            name: "INI",
            alternate_names: &[],
            extensions: vec!["ini"],
            file_names: vec![],
            language: tree_sitter_ini::LANGUAGE.into(),
            atomic_nodes: vec![],
            commutative_parents: vec![
                CommutativeParent::without_delimiters("section", "\n")
                    .restricted_to_groups(&[&["setting"]]),
            ],
            signatures: vec![signature("setting", vec![vec![ChildType("setting_name")]])],
            injections: None,
        },
        LangProfile {
            name: "Javascript",
            alternate_names: &[],
            extensions: vec!["js", "jsx", "mjs"],
            file_names: vec![],
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
            injections: Some(tree_sitter_javascript::INJECTIONS_QUERY),
        },
        LangProfile {
            name: "JSON",
            alternate_names: &[],
            extensions: vec!["json"],
            file_names: vec![],
            language: tree_sitter_json::LANGUAGE.into(),
            atomic_nodes: vec![],
            commutative_parents: vec![
                // the order of keys is deemed irrelevant
                CommutativeParent::new("object", "{", ", ", "}"),
            ],
            signatures: vec![signature("pair", vec![vec![Field("key")]])],
            injections: None,
        },
        LangProfile {
            name: "YAML",
            alternate_names: &[],
            extensions: vec!["yml", "yaml"],
            file_names: vec![],
            language: tree_sitter_yaml::LANGUAGE.into(),
            atomic_nodes: vec!["single_quote_scalar", "double_quote_scalar"],
            commutative_parents: vec![CommutativeParent::without_delimiters("block_mapping", "\n")],
            signatures: vec![signature("block_mapping_pair", vec![vec![Field("key")]])],
            injections: None,
        },
        LangProfile {
            name: "TOML",
            alternate_names: &[],
            extensions: vec!["toml"],
            file_names: vec![],
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
            injections: None,
        },
        LangProfile {
            name: "HTML",
            alternate_names: &[],
            extensions: vec!["html", "htm"],
            file_names: vec![],
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
            injections: Some(tree_sitter_html::INJECTIONS_QUERY),
        },
        LangProfile {
            name: "XML",
            alternate_names: &[],
            extensions: vec!["xhtml", "xml"],
            file_names: vec![],
            language: tree_sitter_xml::LANGUAGE_XML.into(),
            atomic_nodes: vec!["AttValue"],
            commutative_parents: vec![
                CommutativeParent::new("EmptyElemTag", "<", " ", "/>"),
                CommutativeParent::new("STag", "<", " ", ">"),
            ],
            signatures: vec![signature("Attribute", vec![vec![ChildType("Name")]])],
            injections: None,
        },
        LangProfile {
            name: "C/C++",
            alternate_names: &["C", "C++"],
            extensions: vec![
                "c", "h", "cc", "hh", "cpp", "hpp", "cxx", "hxx", "c++", "h++", "mpp", "cppm",
                "ixx", "tcc",
            ],
            file_names: vec![],
            language: tree_sitter_cpp::LANGUAGE.into(),
            atomic_nodes: vec![],
            commutative_parents: vec![
                CommutativeParent::new("initializer_list", "{", ",", "}")
                    .restricted_to_groups(&[&["initializer_pair"]]),
                CommutativeParent::new("field_declaration_list", "{\n", "\n", "\n}\n")
                    .restricted_to(vec![
                        ChildrenGroup::new(&["field_declaration"]),
                        ChildrenGroup::with_separator(&["function_definition"], "\n\n"),
                    ]),
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
            injections: None,
        },
        LangProfile {
            name: "C#",
            alternate_names: &["CSharp"],
            extensions: vec!["cs"],
            file_names: vec![],
            language: tree_sitter_c_sharp::LANGUAGE.into(),
            atomic_nodes: vec![],
            commutative_parents: vec![
                CommutativeParent::without_delimiters("compilation_unit", "\n"),
                CommutativeParent::new("declaration_list", "{", "\n", "}").restricted_to_groups(&[
                    &["using_directive"],
                    &[
                        "field_declaration",
                        "property_declaration",
                        "event_declaration",
                        "event_field_declaration",
                    ],
                    &[
                        "class_declaration",
                        "struct_declaration",
                        "enum_declaration",
                        "delegate_declaration",
                        "method_declaration",
                        "record_declaration",
                        "constructor_declaration",
                        "destructor_declaration",
                        "indexer_declaration",
                        "interface_declaration",
                        "namespace_declaration",
                        "operator_declaration",
                        "conversion_operator_declaration",
                    ],
                ]),
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
            injections: None,
        },
        LangProfile {
            name: "Dart",
            alternate_names: &[],
            extensions: vec!["dart"],
            file_names: vec![],
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
            injections: None,
        },
        LangProfile {
            name: "Devicetree Source",
            alternate_names: &[],
            extensions: vec!["dts"],
            file_names: vec![],
            language: tree_sitter_devicetree::LANGUAGE.into(),
            atomic_nodes: vec!["string_literal"],
            commutative_parents: vec![CommutativeParent::new("node", "{", "\n", "}")],
            signatures: vec![
                signature("property", vec![vec![Field("name")]]),
                signature("node", vec![vec![Field("name")]]),
            ],
            injections: None,
        },
        LangProfile {
            name: "Scala",
            alternate_names: &[],
            extensions: vec!["scala", "sbt"],
            file_names: vec![],
            language: tree_sitter_scala::LANGUAGE.into(),
            atomic_nodes: vec![],
            commutative_parents: vec![],
            signatures: vec![],
            injections: None,
        },
        LangProfile {
            name: "Typescript",
            alternate_names: &[],
            extensions: vec!["ts"],
            file_names: vec![],
            language: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            atomic_nodes: vec![],
            commutative_parents: typescript_commutative_parents,
            signatures: typescript_signatures,
            injections: None,
        },
        LangProfile {
            name: "Typescript (TSX)",
            alternate_names: &[],
            extensions: vec!["tsx"],
            file_names: vec![],
            language: tree_sitter_typescript::LANGUAGE_TSX.into(),
            atomic_nodes: vec![],
            commutative_parents: tsx_commutative_parents,
            signatures: tsx_signatures,
            injections: None,
        },
        LangProfile {
            name: "Python",
            alternate_names: &[],
            extensions: vec!["py"],
            file_names: vec![],
            language: tree_sitter_python::LANGUAGE.into(),
            atomic_nodes: vec!["string", "dotted_name"],
            commutative_parents: vec![
                CommutativeParent::without_delimiters("module", "\n").restricted_to_groups(&[
                    &["import_statement", "import_from_statement"],
                    &["class_definition"],
                ]),
                CommutativeParent::without_delimiters("block", "\n\n").restricted_to_groups(&[&[
                    "function_definition",
                    "decorated_definition",
                    "class_definition",
                ]]),
                CommutativeParent::without_delimiters("import_from_statement", ", ")
                    .restricted_to_groups(&[&["dotted_name"]]),
                CommutativeParent::new("argument_list", "(", ", ", ")")
                    .restricted_to_groups(&[&["keyword_argument"]]),
                CommutativeParent::new("set", "{", ", ", "}"),
                CommutativeParent::from_query(
                    r#"(expression_statement (assignment
   left: (identifier) @variable (#eq? @variable "__all__")
   right: (list) @commutative)
 )"#,
                    "[",
                    ", ",
                    "]",
                ),
            ],
            signatures: vec![
                signature("import_from_statement", vec![vec![]]),
                signature("class_definition", vec![vec![Field("name")]]),
                signature("function_definition", vec![vec![Field("name")]]),
                signature(
                    "decorated_definition",
                    vec![vec![Field("definition"), Field("name")]],
                ),
                signature("dotted_name", vec![vec![]]),
                signature("keyword_argument", vec![vec![Field("name")]]),
                signature("string", vec![vec![]]), // for elements of __all__ lists
            ],
            injections: None,
        },
        LangProfile {
            name: "PHP",
            alternate_names: &[],
            extensions: vec!["php", "phtml"],
            file_names: vec![],
            language: tree_sitter_php::LANGUAGE_PHP.into(),
            // optional settings, explained below
            atomic_nodes: vec![],
            commutative_parents: vec![
                // TODO: allow commutation between "use" and "require" statements, which is
                // currently not possible as "require" statements appear as "expression_statement",
                // which encompasses non-declarative statements too.
                CommutativeParent::without_delimiters("program", "\n")
                    .restricted_to_groups(&[&["namespace_use_declaration"]]),
                CommutativeParent::new("declaration_list", "{", "\n\n", "}").restricted_to_groups(
                    &[
                        &["use_declaration"],
                        &[
                            "const_declaration",
                            "property_declaration",
                            "method_declaration",
                        ],
                    ],
                ),
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
            injections: Some(tree_sitter_php::INJECTIONS_QUERY),
        },
        LangProfile {
            name: "Solidity",
            alternate_names: &[],
            extensions: vec!["sol"],
            file_names: vec![],
            language: tree_sitter_solidity::LANGUAGE.into(),
            atomic_nodes: vec![],
            commutative_parents: vec![
                CommutativeParent::without_delimiters("source_file", "\n"),
                CommutativeParent::without_delimiters("contract_body", "\n"),
            ],
            signatures: vec![],
            injections: None,
        },
        LangProfile {
            name: "Lua",
            alternate_names: &[],
            extensions: vec!["lua"],
            file_names: vec![],
            language: tree_sitter_lua::LANGUAGE.into(),
            atomic_nodes: vec![],
            commutative_parents: vec![],
            signatures: vec![],
            injections: Some(tree_sitter_lua::INJECTIONS_QUERY),
        },
        LangProfile {
            name: "Ruby",
            alternate_names: &[],
            extensions: vec!["rb"],
            file_names: vec![],
            language: tree_sitter_ruby::LANGUAGE.into(),
            atomic_nodes: vec![],
            commutative_parents: vec![],
            signatures: vec![],
            injections: None,
        },
        LangProfile {
            name: "Elixir",
            alternate_names: &[],
            extensions: vec!["ex", "exs"],
            file_names: vec![],
            language: tree_sitter_elixir::LANGUAGE.into(),
            atomic_nodes: vec![],
            commutative_parents: vec![],
            signatures: vec![],
            injections: Some(tree_sitter_elixir::INJECTIONS_QUERY),
        },
        LangProfile {
            name: "Nix",
            alternate_names: &[],
            extensions: vec!["nix"],
            file_names: vec![],
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
            injections: None,
        },
        LangProfile {
            name: "SystemVerilog",
            alternate_names: &[],
            extensions: vec!["sv", "svh"],
            file_names: vec![],
            language: tree_sitter_systemverilog::LANGUAGE.into(),
            atomic_nodes: vec![],
            commutative_parents: vec![],
            signatures: vec![],
            injections: None,
        },
        LangProfile {
            name: "Markdown",
            alternate_names: &[],
            extensions: vec!["md"],
            file_names: vec![],
            language: tree_sitter_md::LANGUAGE.into(),
            atomic_nodes: vec![
                "paragraph_repeat1",
                "link_label",
                "link_destination",
                "link_title",
                "code_fence_content",
                "_line",
                "indented_code_block",
                "pipe_table_delimiter_cell",
                "pipe_table_cell",
            ],
            commutative_parents: vec![
                CommutativeParent::without_delimiters("document_repeat1", "\n\n").restricted_to(
                    vec![ChildrenGroup::with_separator(
                        &["link_reference_definition"],
                        "\n",
                    )],
                ),
            ],
            signatures: vec![],
            injections: Some(tree_sitter_md::INJECTION_QUERY_BLOCK),
        },
        LangProfile {
            name: "HCL",
            alternate_names: &[],
            extensions: vec!["hcl", "tf", "tfvars"],
            file_names: vec![],
            language: tree_sitter_hcl::LANGUAGE.into(),
            atomic_nodes: vec![],
            commutative_parents: vec![],
            signatures: vec![],
            injections: None,
        },
        LangProfile {
            name: "OCaml",
            alternate_names: &[],
            extensions: vec!["ml"],
            file_names: vec![],
            language: tree_sitter_ocaml::LANGUAGE_OCAML.into(),
            atomic_nodes: vec![],
            commutative_parents: ocaml_commutative_parents.clone(),
            signatures: ocaml_signatures.clone(),
            injections: None,
        },
        LangProfile {
            name: "OCaml interfaces",
            alternate_names: &[],
            extensions: vec!["mli"],
            file_names: vec![],
            language: tree_sitter_ocaml::LANGUAGE_OCAML_TYPE.into(),
            atomic_nodes: vec![],
            commutative_parents: ocaml_commutative_parents,
            signatures: ocaml_signatures,
            injections: None,
        },
        LangProfile {
            name: "Haskell",
            alternate_names: &[],
            extensions: vec!["hs"],
            file_names: vec![],
            language: tree_sitter_haskell::LANGUAGE.into(),
            atomic_nodes: vec![],
            commutative_parents: vec![
                CommutativeParent::without_delimiters("imports", "\n"),
                CommutativeParent::new("import_list", "(", ", ", ")"),
                CommutativeParent::new("record", "{", ",\n", "}")
                    .restricted_to_groups(&[&["field_update"]]),
                CommutativeParent::from_query("(deriving (tuple) @commutative)", "(", ", ", ")"),
            ],
            signatures: vec![
                signature("field_update", vec![vec![Field("field")]]),
                signature("field", vec![vec![Field("name")]]),
                signature("import_name", vec![vec![]]),
                signature("signature", vec![vec![Field("name")]]),
                signature("name", vec![vec![]]),
            ],
            injections: None,
        },
    ]
});

#[cfg(test)]
mod test {
    use itertools::Itertools;

    use super::*;

    #[test]
    fn extensions_do_not_start_with_a_dot() {
        for lang_profile in &*SUPPORTED_LANGUAGES {
            for ext in &lang_profile.extensions {
                assert!(!ext.starts_with('.'), "{ext}");
            }
        }
    }

    #[test]
    fn language_names_are_all_distinct() {
        assert!(
            SUPPORTED_LANGUAGES
                .iter()
                .map(|profile| profile.name)
                .all_unique()
        );
    }

    #[test]
    fn injections_are_non_empty() {
        for lang_profile in &*SUPPORTED_LANGUAGES {
            if let Some(injection) = lang_profile.injections {
                assert!(
                    !injection.trim().is_empty(),
                    "Injection query for language {lang_profile} set as an empty string, use None instead"
                );
            }
        }
    }
}
