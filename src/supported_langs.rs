use crate::{
    lang_profile::{CommutativeParent, LangProfile},
    signature::{
        signature,
        PathStep::{ChildType, Field},
    },
};

/// Returns the list of supported language profiles,
/// which contain all the language-specific information required to merge files in that language.
pub fn supported_languages() -> Vec<LangProfile> {
    vec![
        LangProfile {
            name: "Java",
            extensions: vec![".java"],
            language: tree_sitter_java::LANGUAGE.into(),
            atomic_nodes: vec!["import_declaration"],
            commutative_parents: vec![
                // top-level node, for imports and class declarations
                CommutativeParent::without_delimiters("program", "\n"),
                // strictly speaking, this isn't true (order can be accessed via reflection)
                CommutativeParent::new("class_body", " {\n", "\n", "\n}\n"),
                // TODO this encompasses both "public / static / final" sort of things (generally separated by a space) and annotations (separated by newlines)
                CommutativeParent::without_delimiters("modifiers", " "),
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
            name: "Rust",
            extensions: vec![".rs"],
            language: tree_sitter_rust::LANGUAGE.into(),
            atomic_nodes: vec![],
            commutative_parents: vec![
                CommutativeParent::without_delimiters("source_file", "\n"),
                // module members, impls…
                CommutativeParent::new("declaration_list", " {\n", "\n", "\n}\n"),
                // scoped "use" declaration
                CommutativeParent::new("use_list", "{", ", ", "}"),
                CommutativeParent::with_left_delimiter("trait_bounds", ": ", " + "),
                // strictly speaking, the derived order on values depends on their declaration
                CommutativeParent::new("enum_variant_list", " {\n", ", ", "\n}\n"),
                // strictly speaking, the order can matter if using the C ABI
                CommutativeParent::new("field_declaration_list", " {\n", ", ", "\n}\n"),
                CommutativeParent::new("field_initializer_list", "{ ", ", ", " }"),
                CommutativeParent::without_delimiters("function_modifiers", " "),
                CommutativeParent::with_left_delimiter("where_clause", "where", ",\n"),
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
            extensions: vec![".go"],
            language: tree_sitter_go::LANGUAGE.into(),
            atomic_nodes: vec!["interpreted_string_literal"], // for https://github.com/tree-sitter/tree-sitter-go/issues/150
            commutative_parents: vec![
                CommutativeParent::without_delimiters("source_file", "\n"),
                CommutativeParent::new("import_spec_list", "(\n", "\n", "\n)\n"),
                CommutativeParent::new("field_declaration_list", " {\n", "\n", "\n}\n"), // not strictly speaking, because it impacts memory layout
                CommutativeParent::new("literal_value", "{", ", ", "}"),
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
            extensions: vec![".js", ".jsx"],
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
            extensions: vec![".json"],
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
            extensions: vec![".yml", ".yaml"],
            language: tree_sitter_yaml::language(),
            atomic_nodes: vec![],
            commutative_parents: vec![CommutativeParent::without_delimiters("block_mapping", "\n")],
            signatures: vec![signature("block_mapping_pair", vec![vec![Field("key")]])],
        },
        LangProfile {
            name: "HTML",
            extensions: vec![".html", ".htm"],
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
            extensions: vec![".xhtml", ".xml"],
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
            extensions: vec![".c", ".h", ".cc", ".cpp", ".hpp"],
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
            extensions: vec![".cs"],
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
            extensions: vec![".dart"],
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
            name: "Scala",
            extensions: vec![".scala", ".sbt"],
            language: tree_sitter_scala::LANGUAGE.into(),
            atomic_nodes: vec![],
            commutative_parents: vec![],
            signatures: vec![],
        },
        LangProfile {
            name: "Typescript",
            extensions: vec![".ts"],
            language: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            atomic_nodes: vec![],
            commutative_parents: vec![
                CommutativeParent::without_delimiters("program", "\n")
                    .restricted_to_groups(&[&["import_statement"]]),
                CommutativeParent::new("named_imports", "{", ", ", "}"),
                CommutativeParent::new("object", "{", ", ", "}"),
                CommutativeParent::new("class_body", " {\n", "\n\n", "\n}\n"),
                CommutativeParent::new("interface_body", " {\n", ";\n", "\n}\n"),
                CommutativeParent::new("object_type", " {\n", ";\n", "\n}\n"),
                CommutativeParent::new("enum_body", " {\n", ",\n", "\n}\n"),
                CommutativeParent::new("object_pattern", "{", ", ", "}"),
            ],
            signatures: vec![
                signature("import_specifier", vec![vec![Field("name")]]),
                signature("pair", vec![vec![Field("key")]]),
                signature("identifier", vec![vec![]]),
                signature("method_definition", vec![vec![Field("name")]]),
                signature("public_field_definition", vec![vec![Field("name")]]),
                signature("property_signature", vec![vec![Field("name")]]),
                signature("property_identifier", vec![vec![]]),
                signature("pair_pattern", vec![vec![Field("key")]]),
            ],
        },
    ]
}
