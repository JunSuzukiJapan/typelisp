# Graph Report - .  (2026-07-28)

## Corpus Check
- 112 files · ~368,899 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 3669 nodes · 10855 edges · 145 communities (132 shown, 13 thin omitted)
- Extraction: 99% EXTRACTED · 1% INFERRED · 0% AMBIGUOUS · INFERRED: 127 edges (avg confidence: 0.79)
- Token cost: 232,931 input · 0 output

## Community Hubs (Navigation)
- AST to Compile Bridge
- Format and Pretty Printer
- Fasl Serialization and Projects
- Interp Compile Registry
- Interp Numeric Builtins
- Runtime Scope Values
- Type Registry and Assoc Fns
- Compile Dispatch Tests
- Evaluator Tests
- Compiled Closure Tests
- Checker Module Navigation
- LLVM Builder Builtins
- Source Locations and Typed AST
- Error Types and Display
- GC Heap Core
- Runtime Shim Functions
- Macro Tests
- Name Lexer and Type Parsing
- VS Code Extension Sources
- Heap Cons and HashTable Ops
- Cross-Module Macro Tests
- Reader Tests
- Generic Header Checking
- AOT Compilation
- Pretty Printer Tests
- Bignum and Ratio Tests
- Typed AST Nodes
- Format Directive Tests
- Sequence Operation Tests
- Reader Cursor
- Defstruct Tests
- Monomorphization Tests
- Namespace Resolution Tests
- Runtime Encode Tests
- As Conversion Tests
- Error Trait Tests
- Compile File Tests
- Interp Parse and Read
- LSP Diagnostics
- Scope Frame Memory Tests
- Numeric Tests
- String and Char Tests
- Module Scope Tree
- Defenum Tests
- Runtime Closure GC Tests
- Compiled Generic Bound Tests
- Symbol and Path Interning
- Runtime Enum Construction
- LSP Completion Tests
- Dyn Dispatch Tests
- Scope Sharing Tests
- LSP Server Loop
- HashTable Tests
- Trait Mechanism Tests
- Goto Definition Tests
- Dyn Box GC Tests
- Sexpr Downcast Tests
- Grammar Capture Rules
- TypeScript Build Config
- Break and Loop Checking Tests
- LSP Locate and Hover
- Heap Allocation API
- Closure JIT Tests
- LLVM Module Building Tests
- Sexpr Match Tests
- Runtime Cons and Rooting
- Keyword Symbol Tests
- Extension Manifest
- Checker Program Tests
- Redefinition Policy Tests
- Checker Error Recovery
- Dynamic Dispatch Design
- Builtin Definition Flags
- Dyn Match Tests
- Generic Defun Tests
- Cons Reference Values
- Collection Compile Plan
- LSP Smoke Test Script
- Dolist Tests
- Compile Feature Docs
- Extension Settings
- Extension Dev Dependencies
- Comparison Operator Tests
- Method Dispatch Tests
- GC Tracing Tests
- Runtime Global Variables
- Heap and Sexpr Design
- Eval Builtin Tests
- Prelude Error Tests
- Grammar Type Rules
- Interp Ratio Builtins
- Trait and Method Design
- Module Visibility Design
- Naming and Iter Design
- Extension Trace Setting
- Print Object Design
- Pattern Checking
- Island Self Compile Tests
- Namespace and Macro Design
- Extension Contributions
- Extension Build Scripts
- Sexpr Construction Tests
- Island AOT Load Tests
- Grammar Root Definition
- Grammar Comment Blocks
- Grammar String Rules
- Option Result Helper Tests
- Mem Error Construction
- Mem Error Formatting
- Language Server Path Setting
- Extension Repository Metadata
- Grammar Line Comments
- Loop Type Checking Tests
- CL Equivalence Catalog
- Extension Runtime Dependency
- Grammar Builtin Functions
- Grammar Char Literals
- Grammar Clause Keywords
- Grammar Constants
- Grammar Global Variables
- Grammar Keyword Symbols
- Grammar Lambda List
- Grammar Never Type
- Grammar Numbers
- Grammar Special Forms
- Grammar User Types
- Serial Test Script
- LLVM Environment Script
- Defenum Design
- Defstruct Design
- Rest Args and Apply Design
- Island Regeneration Script
- Cargo Environment Script
- Vector Method Proposal

## God Nodes (most connected - your core abstractions)
1. `Heap` - 447 edges
2. `RtValue` - 295 edges
3. `Value` - 291 edges
4. `EvalError` - 231 edges
5. `Checker` - 207 edges
6. `Typed` - 137 edges
7. `Interp` - 120 edges
8. `active_heap()` - 95 edges
9. `fatal()` - 94 edges
10. `Loc` - 90 edges

## Surprising Connections (you probably didn't know these)
- `jit_compiled_code_sees_the_heap_registered_by_set_active_heap()` --calls--> `set_active_heap()`  [INFERRED]
  src/compile/mod.rs → crates/typelisp-rt/src/lib.rs
- `fasl_loaded_prelude_offers_the_same_completions()` --calls--> `completion_candidates()`  [INFERRED]
  tests/fasl_test.rs → src/check/locate.rs
- `expect_bool()` --references--> `RtValue`  [EXTRACTED]
  tests/compile_test.rs → src/eval/value.rs
- `eval_under_gc_pressure()` --references--> `RtValue`  [EXTRACTED]
  tests/eval_test.rs → src/eval/value.rs
- `run_project()` --calls--> `find_src_root()`  [INFERRED]
  tests/macro_use_test.rs → src/project.rs

## Import Cycles
- 2-file cycle: `src/eval/format.rs -> src/eval/pprint.rs -> src/eval/format.rs`
- 3-file cycle: `src/eval/format.rs -> src/eval/interp.rs -> src/eval/pprint.rs -> src/eval/format.rs`

## Hyperedges (group relationships)
- **trait オブジェクト :dyn Trait の一式（表記・表現・安全性・実装）** — docs_dev_language_design_dyn_trait_object, docs_dev_language_design_vtable_fat_box, docs_dev_language_design_object_safety, docs_dev_language_design_dyn_box_transparency, docs_dev_implementation_log_dyn_dispatch, docs_dev_implementation_log_vtable_slot_order, docs_syntax_dyn_trait_type [EXTRACTED 1.00]
- **Symbol 型導入と Sexpr の裏方化（island 層への移行）** — docs_dev_symbol_sexpr_redesign_symbol_first_class, docs_dev_symbol_sexpr_redesign_sexpr_island, docs_dev_symbol_sexpr_redesign_cons_cell_generic_pair, docs_dev_symbol_sexpr_redesign_match_fence, docs_functions_sexpr_accessors, docs_functions_cons_cell, docs_dev_language_design_symbol_type [EXTRACTED 1.00]
- **コレクション・プリミティブ層の compile 対応パイプライン** — docs_dev_iter_compile_plan_vector_op, docs_dev_iter_compile_plan_hashtable_op, docs_dev_iter_compile_plan_transitive_compile_driver, docs_dev_iter_compile_plan_alloca_branch_merge, docs_dev_iter_compile_plan_monomorphization_removes_dispatch, docs_dev_language_design_compile_jit_aot [EXTRACTED 1.00]

## Communities (145 total, 13 thin omitted)

### Community 0 - "AST to Compile Bridge"
Cohesion: 0.05
Nodes (157): Fn, a_field_index_of_zero_encodes_as_an_empty_list(), a_quoted_path_compiles(), a_quoted_symbol_nested_in_a_cons_compiles(), a_reference_to_a_captured_name_inside_the_capturing_lambda_becomes_cellvar(), a_sexpr_typed_field_of_a_general_adt_construct_is_tagged_kind_six(), an_apply_to_a_name_outside_the_current_labels_scope_is_indirect(), an_fn_typed_if_carries_an_is_fn_tag() (+149 more)

### Community 1 - "Format and Pretty Printer"
Cohesion: 0.06
Nodes (85): From, Items, apply_case(), build(), capitalize_first(), capitalize_words(), char_name(), current_column() (+77 more)

### Community 2 - "Fasl Serialization and Projects"
Cohesion: 0.05
Nodes (85): AdtDef, FnSig, FsPath, DefLocsRepr, diff_namespace(), Fasl, FnTemplateRepr, MethodTemplateRepr (+77 more)

### Community 3 - "Interp Compile Registry"
Cohesion: 0.07
Nodes (32): is_builtin_error_type(), CallEdge, collect_sexpr_roots(), compile_function_compiles_a_genuine_two_node_cycle_bypassing_the_checker(), declare_external_function(), EnumDef, env_get(), fn_path_from_node_name() (+24 more)

### Community 4 - "Interp Numeric Builtins"
Cohesion: 0.07
Nodes (85): bignum_to_float(), bignum_to_int(), bignum_to_ratio(), bool_eq(), char_compare(), char_eq(), char_eqp(), char_lt() (+77 more)

### Community 5 - "Runtime Scope Values"
Cohesion: 0.04
Nodes (62): ScopeFrame, Step, NativeScope, BasicBlock, BasicValueEnum, BigInt, BigRational, Builder (+54 more)

### Community 6 - "Type Registry and Assoc Fns"
Cohesion: 0.07
Nodes (70): AdtDef, AdtKind, assoc_fn(), AssocFn, bignum_assoc(), bool_assoc(), builtin_error_defs(), char_assoc() (+62 more)

### Community 7 - "Compile Dispatch Tests"
Cohesion: 0.05
Nodes (80): a_variadic_function_compiles_and_dispatches_to_native_code(), compile_and_interpret_agree_on_a_trait_object_match(), compile_constructs_a_bignum_literal_and_agrees_with_the_interpreter(), compile_constructs_a_ratio_literal_and_agrees_with_the_interpreter(), compile_dispatches_a_defstruct_field_accessor_method_to_native_code(), compile_dispatches_a_defstruct_field_of_fn_type_to_native_code(), compile_dispatches_a_defstruct_field_of_option_type_to_native_code(), compile_dispatches_a_defstruct_field_setter_method_to_native_code() (+72 more)

### Community 10 - "Compiled Closure Tests"
Cohesion: 0.03
Nodes (77): a_sibling_that_never_references_a_capture_still_forwards_it_to_another_sibling(), an_uncompiled_function_still_tree_walks_normally(), compile_a_compiled_produced_closure_survives_interp_apply_then_crosses_into_another_compiled_call(), compile_a_same_named_method_in_two_sibling_modules_does_not_alias_the_others_llvm_symbol(), compile_a_setf_on_a_captured_name_is_visible_on_the_next_call_through_the_same_closure(), compile_a_setf_through_one_labels_sibling_is_visible_through_another_sharing_the_same_capture(), compile_and_interpret_agree_on_a_sexpr_match(), compile_bare_name_prefers_the_callers_own_module_over_a_same_named_sibling() (+69 more)

### Community 11 - "Checker Module Navigation"
Cohesion: 0.07
Nodes (19): Deserialize, Serialize, Checker, FnTemplate, MacroShape, MethodTemplate, NsContext, RedefPolicy (+11 more)

### Community 12 - "LLVM Builder Builtins"
Cohesion: 0.10
Nodes (65): BuilderError, FloatPredicate, FloatValue, FunctionType, IntPredicate, IntValue, alloc_quoted(), compiled_fn_type() (+57 more)

### Community 13 - "Source Locations and Typed AST"
Cohesion: 0.16
Nodes (12): Loc, RestParam, PartialEq, Self, Typed, Env, MacroExpander, nth_loc() (+4 more)

### Community 14 - "Error Types and Display"
Cohesion: 0.11
Nodes (19): Error, Box, Display, Option, String, Vec, ExportedTemplates, join_types() (+11 more)

### Community 15 - "GC Heap Core"
Cohesion: 0.06
Nodes (5): Heap, HashMap, Weak, BoxId, Drop

### Community 16 - "Runtime Shim Functions"
Cohesion: 0.08
Nodes (58): active_heap(), decode(), fatal(), ratio_arg(), ratio_pair(), BigRational, rt_atom(), rt_box_kind() (+50 more)

### Community 17 - "Macro Tests"
Cohesion: 0.07
Nodes (48): as_sexpr_string(), as_sexpr_string_of(), basic_conditional_macro(), cond_with_no_matching_clause_and_no_else_is_unit(), eval_ok(), eval_ok_with_prelude(), gensym_fixes_macro_hygiene(), gensym_is_fresh_each_call() (+40 more)

### Community 18 - "Name Lexer and Type Parsing"
Cohesion: 0.06
Nodes (27): D, Ok, Peekable, NameLexer, NameLexer<'a>, NameTok, Iterator, Option (+19 more)

### Community 19 - "VS Code Extension Sources"
Cohesion: 0.07
Nodes (46): activate(), deactivate(), documentSymbolProvider, formattingProvider, onTypeFormattingProvider, quoteForShell(), rangeFormattingProvider, resolveProgram() (+38 more)

### Community 21 - "Heap Cons and HashTable Ops"
Cohesion: 0.06
Nodes (21): Option, Result, Vec, Value, global_new(), is_symbol(), is_unsupported_tag(), eql_val() (+13 more)

### Community 22 - "Cross-Module Macro Tests"
Cohesion: 0.08
Nodes (47): cross_module_macro_call_at_top_level(), cross_module_macro_call_in_expression_position(), cross_module_macro_call_respects_visibility(), cross_module_macro_generating_a_quoted_path_to_a_third_module(), failing_macro_expansion_reports_at_check_time(), macro_chain_ending_in_use(), macro_generated_defun_defines_a_callable_function(), macro_generated_module_containing_use() (+39 more)

### Community 23 - "Reader Tests"
Cohesion: 0.08
Nodes (39): a_generic_type_token_may_contain_a_spaced_dyn_argument(), a_leading_angle_bracket_is_still_the_comparison_operator(), an_operator_name_ending_in_an_angle_bracket_still_reads_as_three_data(), an_unbalanced_angle_bracket_rewinds_rather_than_swallowing_the_input(), booleans(), characters(), dot_inside_token_is_not_a_dotted_pair(), dotted_pair() (+31 more)

### Community 24 - "Generic Header Checking"
Cohesion: 0.09
Nodes (25): as_conversion(), AssocCall, float_lit_ty(), int_lit_ty(), is_boxable_scalar(), is_param(), is_self_tvar(), mangle_type() (+17 more)

### Community 25 - "AOT Compilation"
Cohesion: 0.08
Nodes (37): CompiledSignature, JitFunction, aot_main_wrapper_initializes_a_heap_before_tl_main_runs(), aot_output_can_call_an_rt_extern_function(), build_main_wrapper(), collect_aot_item(), compile_file(), Context (+29 more)

### Community 26 - "Pretty Printer Tests"
Cohesion: 0.07
Nodes (26): dropping_the_cell_handle_makes_cell_and_contents_collectable(), a_break_out_of_a_block_still_flushes_what_it_printed(), a_custom_representation_composes_with_the_pretty_printer(), a_logical_block_collects_ordinary_print_calls(), a_logical_block_supports_a_per_line_prefix(), a_method_named_print_object_that_is_not_the_trait_is_ignored(), a_non_literal_block_prefix_is_rejected(), a_print_object_that_conses_heavily_survives_collections() (+18 more)

### Community 27 - "Bignum and Ratio Tests"
Cohesion: 0.10
Nodes (30): a_negative_bignum_literal_reads_correctly(), a_negative_ratio_literal_normalizes_the_sign_onto_the_numerator(), a_ratio_literal_reads_in_reduced_form(), an_integer_literal_past_i64_range_reads_as_a_bignum(), assert_bignum(), assert_ratio(), bignum(), bignum_abs_and_signum() (+22 more)

### Community 28 - "Typed AST Nodes"
Cohesion: 0.10
Nodes (29): Arm, CompileTarget, Expr, MacroLambda, Pattern, QuotedSexpr, Ref, BigInt (+21 more)

### Community 29 - "Format Directive Tests"
Cohesion: 0.05
Nodes (6): fmt(), Result, String, run(), too_few_arguments_is_a_recoverable_error(), unknown_directive_is_an_error()

### Community 30 - "Sequence Operation Tests"
Cohesion: 0.05
Nodes (6): append_on_strings_still_resolves_to_the_builtin_method(), check(), eval_ok(), pair_equals_rejects_an_element_type_without_eq(), Result, run()

### Community 31 - "Reader Cursor"
Cohesion: 0.17
Nodes (25): Cursor, extend_angle_token(), is_delim_or_eof(), is_delimiter(), is_ws(), parse_number(), read_atom(), read_char() (+17 more)

### Community 32 - "Defstruct Tests"
Cohesion: 0.05
Nodes (7): check(), defstruct_registers_a_type(), generic_defstruct_constructs_an_instance(), Result, run_with_heap(), struct_new_constructs_an_instance(), two_different_struct_types_do_not_collide()

### Community 33 - "Monomorphization Tests"
Cohesion: 0.08
Nodes (16): a_generic_function_value_without_type_context_is_a_check_error(), a_method_on_a_generic_type_specializes_at_the_call_site(), a_non_generic_call_is_not_bundled(), a_sexpr_instantiated_generic_field_returns_the_datum_not_a_misdecoded_scalar(), a_vector_of_sexpr_element_returns_the_datum(), an_instantiating_call_comes_back_bundled_with_its_specialization(), bundled_specs(), calls_at_different_types_get_independent_specializations() (+8 more)

### Community 34 - "Namespace Resolution Tests"
Cohesion: 0.06
Nodes (3): program(), Result, ty_program()

### Community 35 - "Runtime Encode Tests"
Cohesion: 0.10
Nodes (26): bignum_arg(), bignum_pair(), hashtable_arg(), BigInt, Vec, rt_bignum_add(), rt_bignum_cmp(), rt_bignum_div() (+18 more)

### Community 36 - "As Conversion Tests"
Cohesion: 0.09
Nodes (19): as_converts_f64_to_bignum_and_ratio(), as_narrows_ratio_to_bignum_by_truncating(), as_panics_converting_an_invalid_scalar_int_to_char(), as_panics_narrowing_an_out_of_range_bignum_to_int(), as_rejects_a_type_outside_the_numeric_char_catalog(), as_widens_bignum_to_ratio_and_f64(), as_widens_int_to_bignum_and_ratio(), as_widens_ratio_to_f64() (+11 more)

### Community 37 - "Error Trait Tests"
Cohesion: 0.08
Nodes (19): a_trait_may_not_take_a_types_name(), a_trait_written_in_type_position_is_rejected(), a_type_may_not_take_a_traits_name(), assert_prelude_type_error(), assert_type_error(), error_ty(), match_result_exhaustive_with_panic_arm(), panic_message_must_be_string() (+11 more)

### Community 38 - "Compile File Tests"
Cohesion: 0.09
Nodes (13): compile_and_run(), errors_on_a_non_defun_top_level_form(), errors_without_a_zero_argument_main(), jit_and_aot_agree_on_a_cross_function_call(), jit_and_aot_agree_on_a_defenum_global_read_and_write(), jit_and_aot_agree_on_a_global_read_and_write(), jit_and_aot_agree_on_a_labels_body(), jit_and_aot_agree_on_a_loop_based_function() (+5 more)

### Community 39 - "Interp Parse and Read"
Cohesion: 0.09
Nodes (23): build_enum_value(), eval_parse_float(), eval_parse_int(), eval_random(), eval_read(), int_to_bignum(), int_to_ratio(), next_random_u64() (+15 more)

### Community 40 - "LSP Diagnostics"
Cohesion: 0.09
Nodes (15): Diagnostic, DiagnosticSeverity, Position, Range, candidates_for_offers_locals_inside_a_truncated_non_catchall_match_arm(), char_offset(), diagnostic(), doc_start_range() (+7 more)

### Community 41 - "Scope Frame Memory Tests"
Cohesion: 0.07
Nodes (30): a_fresh_scope_starts_with_one_empty_frame(), a_shared_frame_survives_the_death_of_one_of_its_scopes(), as_boxed(), clone_frames_shares_existing_frames_by_reference(), frames_pushed_after_clone_frames_are_not_shared(), gc_keeps_a_rooted_hashtables_string_keys_alive(), hashtable_accessor_on_a_boxed_float_panics(), hashtable_accessor_on_a_boxed_struct_panics() (+22 more)

### Community 42 - "Numeric Tests"
Cohesion: 0.12
Nodes (16): assert_close(), eval_ok(), f64_abs(), f64_ceiling(), f64_division_by_zero_is_infinity_not_a_panic(), f64_expt(), f64_floor(), f64_mod() (+8 more)

### Community 43 - "String and Char Tests"
Cohesion: 0.08
Nodes (3): char_only_method_on_a_string_is_a_type_error(), string_method_on_wrong_type_is_a_type_error(), type_error()

### Community 44 - "Module Scope Tree"
Cohesion: 0.20
Nodes (10): GlobalDef, in_scope(), ModuleScope, HashMap, HashSet, Option, Rc, String (+2 more)

### Community 45 - "Defenum Tests"
Cohesion: 0.12
Nodes (19): a_bare_constructor_is_unresolved_without_use(), a_bare_symbol_nullary_variant_is_accepted(), a_constructor_arity_mismatch_is_rejected(), a_duplicate_variant_name_is_rejected(), a_non_exhaustive_match_is_rejected(), an_enum_with_no_variants_is_rejected(), assert_check_err(), check() (+11 more)

### Community 46 - "Runtime Closure GC Tests"
Cohesion: 0.10
Nodes (26): a_rooted_compiled_closure_keeps_its_captured_cell_alive_across_gc(), an_unreachable_compiled_closure_is_swept(), encode(), rt_bignum_new(), rt_cell_get(), rt_cell_new(), rt_cell_set_mutates_the_shared_cell_both_worlds_see(), rt_closure_env_get() (+18 more)

### Community 47 - "Compiled Generic Bound Tests"
Cohesion: 0.08
Nodes (26): a_where_bounded_generic_specializes_and_dispatches_into_compiled_methods(), a_where_bounded_generic_specializes_per_impl_and_dispatches_into_compiled_methods(), an_escaping_lambdas_captured_closure_survives_gc_pressure(), compile_a_captured_cell_survives_gc_pressure_across_many_calls(), compile_can_set_a_defenum_global_and_the_interpreter_sees_the_write(), compile_can_set_an_option_typed_global_and_the_interpreter_sees_the_write(), compile_dispatches_a_function_that_keeps_a_let_bound_str_local_rooted_across_many_allocations(), compile_dispatches_a_function_that_keeps_an_enum_scrutinee_rooted_across_many_allocations() (+18 more)

### Community 49 - "Symbol and Path Interning"
Cohesion: 0.12
Nodes (11): BoxedObj, MemHashKey, PathId, BigInt, BigRational, HashMap, String, Vec (+3 more)

### Community 50 - "Runtime Enum Construction"
Cohesion: 0.18
Nodes (24): a_rooted_enum_box_keeps_its_cons_field_alive_across_gc(), gc_reclaims_an_unrooted_structs_fields(), gc_traces_into_a_rooted_structs_fields(), make_str(), rt_data_new(), rt_data_new_builds_a_boxed_enum_with_its_variant_and_fields(), rt_data_new_with_no_fields_builds_a_nullary_variant(), rt_str_append_concatenates_into_a_fresh_string() (+16 more)

### Community 51 - "LSP Completion Tests"
Cohesion: 0.20
Nodes (23): completion_candidates(), completion_locals(), a_private_root_function_is_visible_from_inside_a_submodule(), a_public_root_function_is_visible_from_inside_a_submodule(), a_submodules_own_definition_is_visible_from_inside_it_but_not_from_root(), check(), completion_locals_does_not_leak_a_match_pattern_binding_into_a_sibling_arm(), completion_locals_does_not_leak_a_sibling_lets_binding() (+15 more)

### Community 52 - "Dyn Dispatch Tests"
Cohesion: 0.13
Nodes (14): a_method_returning_self_makes_a_trait_not_object_safe(), a_method_the_trait_does_not_declare_cannot_be_called_on_a_trait_object(), a_mismatched_associated_type_pin_is_rejected(), a_primitive_cannot_be_boxed_even_when_it_implements_the_trait(), a_trait_with_a_static_method_is_not_object_safe(), an_unknown_trait_name_is_reported_as_such(), dyn_in_a_value_position_is_a_type_error(), eval_err() (+6 more)

### Community 53 - "Scope Sharing Tests"
Cohesion: 0.10
Nodes (6): eval_ok(), heap_scope_set_with_every_frame_popped_is_a_catchable_error_not_a_panic(), heap_scope_sexpr_values_survive_gc_pressure(), Result, run(), run_with_capacity_and_prelude()

### Community 54 - "LSP Server Loop"
Cohesion: 0.27
Nodes (23): Connection, RequestId, Response, Analysis, build_overlay(), candidates_for(), diagnostics_for(), handle_completion() (+15 more)

### Community 55 - "HashTable Tests"
Cohesion: 0.11
Nodes (8): eval_ok(), Result, run(), run_with_capacity(), run_with_capacity_and_prelude(), sexpr_values_survive_gc_pressure(), type_error(), wrong_key_type_is_a_type_error()

### Community 56 - "Trait Mechanism Tests"
Cohesion: 0.11
Nodes (6): bounded_method_call_rejects_an_owner_argument_lacking_the_impl(), check(), eval_ok(), forwarding_a_bare_type_parameter_without_a_matching_where_bound_is_a_type_error(), Result, run()

### Community 57 - "Goto Definition Tests"
Cohesion: 0.27
Nodes (18): definition_target(), locate_node(), Option, goto_definition_on_a_labels_function_and_parameter_resolves_to_their_bindings(), goto_definition_on_a_lambda_parameter_reference_resolves_to_the_parameter(), goto_definition_on_a_let_bound_reference_resolves_to_the_binding(), goto_definition_on_a_match_pattern_bound_reference_resolves_to_the_pattern(), goto_definition_on_a_parameter_reference_resolves_to_the_parameter() (+10 more)

### Community 58 - "Dyn Box GC Tests"
Cohesion: 0.10
Nodes (20): a_dyn_box_and_its_value_are_reclaimed_together_once_unrooted(), a_dyn_box_keeps_the_value_it_wraps_alive(), a_dyn_box_reachable_only_through_a_cons_car_survives_gc(), a_live_cell_keeps_its_cons_contents_across_gc(), a_struct_reachable_only_through_a_cons_car_is_reclaimed_once_unrooted(), a_struct_reachable_only_through_a_cons_car_survives_gc(), accounting_holds_through_a_sequence(), assert_accounting() (+12 more)

### Community 59 - "Sexpr Downcast Tests"
Cohesion: 0.18
Nodes (18): a_generic_downcast_pattern_head_is_a_type_error(), a_struct_downcast_pattern_does_not_false_match_a_different_same_shape_struct(), a_struct_downcast_pattern_does_not_false_match_nil(), check(), equalp_distinguishes_structs_by_type_name_even_with_matching_fields(), equalp_recursively_compares_two_distinct_struct_instances(), eval_ok(), list_holds_an_enum_variant() (+10 more)

### Community 61 - "Grammar Capture Rules"
Cohesion: 0.15
Nodes (19): name, name, name, 1, 2, 3, captures, match (+11 more)

### Community 62 - "TypeScript Build Config"
Cohesion: 0.11
Nodes (18): compilerOptions, esModuleInterop, lib, module, moduleResolution, noFallthroughCasesInSwitch, noImplicitReturns, noUnusedLocals (+10 more)

### Community 63 - "Break and Loop Checking Tests"
Cohesion: 0.11
Nodes (19): apply_checks_argument_types(), assert_type_error(), break_does_not_cross_labels_boundary(), break_does_not_cross_lambda_boundary(), break_outside_loop_errors(), break_takes_no_arguments(), calling_a_non_function_errors(), exit_rejects_wrong_arity() (+11 more)

### Community 64 - "LSP Locate and Hover"
Cohesion: 0.23
Nodes (17): CompletionItemKind, completion_item_kind(), CompletionCandidate, CompletionKind, expr_children(), hover_text(), Nearest, pattern_bind_names() (+9 more)

### Community 65 - "Heap Allocation API"
Cohesion: 0.13
Nodes (5): BigInt, BigRational, Rc, String, Cell

### Community 66 - "Closure JIT Tests"
Cohesion: 0.12
Nodes (3): eval_ok(), Result, run()

### Community 67 - "LLVM Module Building Tests"
Cohesion: 0.12
Nodes (18): a_closure_made_from_a_capturing_function_can_be_called_indirectly(), a_function_can_directly_call_another_function_in_the_same_module(), a_function_using_load_arg_and_build_add_computes_correctly(), build_or_and_build_and_pack_and_read_back_a_tag(), build_shl_and_build_ashr_round_trip_a_signed_fixnum_payload(), builds_a_module_with_a_constant_returning_function(), compile_and_interpret_agree_on_a_defenum_match(), compile_and_interpret_agree_on_a_defstruct_match() (+10 more)

### Community 69 - "Runtime Cons and Rooting"
Cohesion: 0.18
Nodes (17): a_setf_reassigned_sexpr_value_is_corrupted_by_a_gc_triggered_by_other_allocations_without_rt_set_sexpr_root(), an_unrooted_value_is_corrupted_by_a_gc_triggered_by_other_allocations(), rt_car(), rt_cdr(), rt_cons(), rt_cons_rt_car_rt_cdr_round_trip_through_a_real_heap(), rt_heap_live_count_reflects_the_registered_heaps_real_state(), rt_pop_sexpr_root() (+9 more)

### Community 70 - "Keyword Symbol Tests"
Cohesion: 0.18
Nodes (10): a_keyword_can_be_stored_in_a_sexpr_datum(), a_keyword_is_typed_as_symbol_and_flows_where_a_symbol_is_expected(), a_keyword_may_not_contain_further_colons(), a_leading_double_colon_is_still_the_absolute_path_syntax(), defmacro_key_arguments_still_parse_as_keywords(), eval_ok(), read_err(), Result (+2 more)

### Community 71 - "Extension Manifest"
Cohesion: 0.14
Nodes (13): activationEvents, categories, description, displayName, engines, vscode, main, name (+5 more)

### Community 72 - "Checker Program Tests"
Cohesion: 0.15
Nodes (14): defun_registers_and_checks_body(), exit_type_checks_as_never(), form(), match_option_exhaustive_unwrap_or(), primary(), program(), program_with_prelude(), Result (+6 more)

### Community 74 - "Redefinition Policy Tests"
Cohesion: 0.24
Nodes (13): redefining_a_builtin_function_is_always_an_error(), redefining_a_builtin_instance_method_is_always_an_error(), redefining_a_user_function_warns_by_default(), redefining_a_user_function_with_error_policy_fails(), redefining_a_user_function_with_silent_policy_has_no_warning(), redefining_a_user_macro_warns_by_default(), redefining_a_user_method_warns_by_default(), redefining_a_user_var_warns_by_default() (+5 more)

### Community 76 - "Checker Error Recovery"
Cohesion: 0.15
Nodes (13): LSP stdio スモークテスト（lsp-recover-smoke.py）, Checker::set_recover（エラー回復モード）検証手順, 回復モードは LSP 専用（CLI/REPL/prelude は厳格）, チェッカーのエラー回復モード実装（6つの回復境界）, Never 型 !（発散）, 静的型付け Lisp という基本方針, Phase 5 の match-on-Sexpr fence とその再解禁, TODO: 残作業（T1〜T5 完了、着手候補なし） (+5 more)

### Community 77 - "Dynamic Dispatch Design"
Cohesion: 0.15
Nodes (13): 動的ディスパッチ実装（interp/JIT/AOT 3経路）, Error トレイト実装と発見された穴3つ, Tarjan SCC ベースの多パス compile パイプライン, TraitDef::method_order がスロット順の唯一の権威, compile_function_rec（単型化インスタンスの推移的自動 compile）, 定義時 JIT によるクロージャ表現統一, trait オブジェクトの箱の透過性, trait オブジェクト :dyn Trait（動的ディスパッチ） (+5 more)

### Community 78 - "Builtin Definition Flags"
Cohesion: 0.15
Nodes (7): AdtDef, AssocFn, Definable, FnSig, MacroDef, TraitDef, VarInfo

### Community 79 - "Dyn Match Tests"
Cohesion: 0.17
Nodes (4): eval_ok(), Result, String, run()

### Community 80 - "Generic Defun Tests"
Cohesion: 0.19
Nodes (6): assert_type_error(), eval_ok(), mismatched_concrete_argument_types_still_rejected(), Result, run(), unused_type_param_is_uninferable()

### Community 81 - "Cons Reference Values"
Cohesion: 0.17
Nodes (6): ConsRef, Formatter, PartialEq, Result, Self, Debug

### Community 82 - "Collection Compile Plan"
Cohesion: 0.18
Nodes (12): ハッシュテーブルをメソッド API として設計する提案, 「念のため根を積む」が有害になる sync_roots の LIFO 不変条件, Vector<T>::pop 実装と Sexpr リスト変換の対象外確定, alloca+分岐+merge による Option 返しメソッドの compile, hashtable-op ノード（HashTable 層の compile）, vector-op ノード（Vector プリミティブ層の compile）, Phase 4b: cons/car/cdr を cons-cell<A,B> へ付け替え, sexpr-* 内部 island 層（read/eval/print/defmacro/compiler.rs） (+4 more)

### Community 83 - "LSP Smoke Test Script"
Cohesion: 0.30
Nodes (11): _await_response(), check(), _drain_notifications(), main(), _notify(), 1メッセージ読む。ヘッダを解釈して本文を返す。EOF なら None。, 指定 id のレスポンスが来るまで読み進め、その間の通知を貯めて一緒に返す。, method 一致の通知が来るまで最大 timeout_msgs 件読む。 (+3 more)

### Community 84 - "Dolist Tests"
Cohesion: 0.18
Nodes (3): check(), dolist_body_match_still_gets_exhaustiveness_checking(), Result

### Community 85 - "Compile Feature Docs"
Cohesion: 0.18
Nodes (11): :dyn を2語にしたことのリーダへの波及, compile / compile-file（LLVM JIT・AOT、self-hosting）, ast_bridge の (unsupported "<Variant>") 設計, scripts/with-llvm-env.sh（LLVM_SYS_170_PREFIX の動的解決）, bignum / ratio（多倍長数値）, compile / compile-file の利用者向け構文, :dyn Trait 型表記, キーワード :name（CL 準拠、自己評価） (+3 more)

### Community 86 - "Extension Settings"
Cohesion: 0.18
Nodes (11): properties, title, configuration, typelisp.languageServer.enable, typelisp.program, default, markdownDescription, type (+3 more)

### Community 87 - "Extension Dev Dependencies"
Cohesion: 0.18
Nodes (11): devDependencies, @types/node, @types/vscode, typescript, vscode-oniguruma, vscode-textmate, @types/node, @types/vscode (+3 more)

### Community 88 - "Comparison Operator Tests"
Cohesion: 0.20
Nodes (3): b(), Result, run()

### Community 89 - "Method Dispatch Tests"
Cohesion: 0.20
Nodes (4): eval_ok(), Result, String, run()

### Community 90 - "GC Tracing Tests"
Cohesion: 0.18
Nodes (11): deep_list_marks_without_stack_overflow(), dropping_the_root_lets_it_be_collected(), gc_is_idempotent(), gc_traces_into_a_rooted_hashtables_values(), gc_traces_into_a_rooted_structs_fields(), gc_traces_through_a_rooted_scopes_frames_into_bound_values(), list_of(), many_heaps_create_and_drop_cleanly() (+3 more)

### Community 91 - "Runtime Global Variables"
Cohesion: 0.29
Nodes (10): global_perm_idx(), reset_global_table(), Option, rt_global_get(), rt_global_ids_are_sequential_regardless_of_interleaved_permanent_roots(), rt_global_new(), rt_global_new_and_rt_global_get_round_trip(), rt_global_protects_its_value_across_a_gc_triggered_by_other_allocations() (+2 more)

### Community 92 - "Heap and Sexpr Design"
Cohesion: 0.24
Nodes (10): --heap-cells N（cons アリーナ容量指定）, pretty printer を XP でなく2パスで実装, 論理ブロックを暗黙のインタプリタ状態にした（Tier2）, 固定 cons アリーナ + mark-sweep GC, 組み込み直和型 Sexpr, Sexpr がユーザ定義 ADT インスタンスを保持する, Symbol は Sexpr とは別の独立プリミティブ型, Phase 0: Symbol 第一級型化 (+2 more)

### Community 93 - "Eval Builtin Tests"
Cohesion: 0.36
Nodes (9): a_definition_is_visible_to_a_direct_call_on_a_later_repl_line(), a_definition_is_visible_to_a_subsequent_eval(), a_global_shadowed_by_a_local_is_still_seen_by_its_global_value(), an_ill_typed_form_is_a_recoverable_err_not_a_panic(), evaluates_an_expression_and_returns_its_value_as_a_sexpr(), evaluates_in_the_null_lexical_environment(), repl_stdout(), String (+1 more)

### Community 94 - "Prelude Error Tests"
Cohesion: 0.20
Nodes (10): car_of_a_non_pair_is_a_type_error(), equal_rejects_mismatched_types(), eval_ok(), FnOnce, R, Result, run(), symbol_to_string_rejects_a_non_symbol_at_check_time() (+2 more)

### Community 95 - "Grammar Type Rules"
Cohesion: 0.22
Nodes (9): match, name, match, name, match, repository, builtin-type, declaration (+1 more)

### Community 96 - "Interp Ratio Builtins"
Cohesion: 0.22
Nodes (9): eval_ratio_builtin(), expect_ratio(), numeric_as_ratio(), ratio_denominator(), ratio_numerator(), ratio_to_bignum(), ratio_to_float(), BigRational (+1 more)

### Community 97 - "Trait and Method Design"
Cohesion: 0.25
Nodes (8): eq/eql/equal/equalp の CL 準拠再設計（訂正）, プリミティブ型への defmethod 拡張（前提となる Rust 拡張）, トレイトディスパッチは compile のブロッカーではない, defmethod（インスタンス/static メソッド機構）, doiter（Iter トレイト経由の反復マクロ）, trait 機構（deftrait / impl / where 境界）, where 境界の伝播（検証側と推論側の非対称な見落とし）, deftrait / impl 構文

### Community 98 - "Module Visibility Design"
Cohesion: 0.25
Nodes (8): メソッド名と自由関数の優先順位（衝突の注意）, モジュール可視性の祖先チェーン方式, Interp のフラットテーブルを ModuleScope ツリーへ移行, fasl（チェック済み状態のシリアライズ）と (load), 裸名・裸 head の解決順序, load（コンパイル済み優先ロード）, pub 公開指定（flat 形式・1定義ずつ）, typelisp-mode のキーバインド（typl CLI 呼び出し）

### Community 99 - "Naming and Iter Design"
Cohesion: 0.29
Nodes (8): 関数・特殊形の名前に ! を一切使わない（§0.2）, dolist（cons セルリスト専用の反復）, 命名規則: !/? を接尾辞に使わない, Phase 6.5: Eq / Ord トレイトの新設, Phase 4a: コンビネータを generic Iter 化, Sexpr への Iter<Item> 実装は意図的に対象外, Eq / Ord トレイト（比較）, Iter 上のシーケンスライブラリ関数

### Community 100 - "Extension Trace Setting"
Cohesion: 0.25
Nodes (8): typelisp.trace.server, default, description, enum, type, messages, off, verbose

### Community 101 - "Print Object Design"
Cohesion: 0.33
Nodes (7): 対象外: 多値・catch/throw・コンディションシステム, print-object の再入ガード（値の同一性で判定）, print-object トレイト（登録は静的・選択は印字時）, Result と panic の住み分け（?/try 非採用）, set-pprint-dispatch を採らない判断, Option<T> / Result<T,E> のヘルパー, print-object トレイト（型ごとの印字表現）

### Community 104 - "Island Self Compile Tests"
Cohesion: 0.48
Nodes (5): compile_function_compiles_transitively(), eval_in(), every_island_defun_compiles(), fresh(), Result

### Community 105 - "Namespace and Macro Design"
Cohesion: 0.33
Nodes (6): CL 流（非衛生的）defmacro と定義順制約, ファイルパス＝モジュールPath（オンデマンドロード）, module は名前空間、型は名前空間ではない, :: を reader が Value::Path へ分割, defmacro 構文（CL 流ラムダリスト）, module / use（名前空間）

### Community 106 - "Extension Contributions"
Cohesion: 0.33
Nodes (6): contributes, commands, grammars, keybindings, languages, problemMatchers

### Community 107 - "Extension Build Scripts"
Cohesion: 0.33
Nodes (6): scripts, compile, package, pretest, test, watch

### Community 108 - "Sexpr Construction Tests"
Cohesion: 0.33
Nodes (6): assert_sexpr_eq(), float_sexpr_constructs_and_extracts_through_a_heap_boxed_value(), list_builds_cons_chain(), FnOnce, sexpr_cons_as_value(), sexpr_cons_car_cdr_mirror_the_user_facing_ops()

### Community 109 - "Island AOT Load Tests"
Cohesion: 0.40
Nodes (3): aot_and_interpreted_island_agree_on_a_compiled_function(), Result, run_with()

### Community 111 - "Grammar Root Definition"
Cohesion: 0.40
Nodes (4): name, patterns, $schema, scopeName

### Community 112 - "Grammar Comment Blocks"
Cohesion: 0.40
Nodes (5): begin, end, name, patterns, comment-block

### Community 113 - "Grammar String Rules"
Cohesion: 0.40
Nodes (5): string, begin, end, name, patterns

### Community 115 - "Option Result Helper Tests"
Cohesion: 0.40
Nodes (5): eval_true(), identity_returns_its_argument_at_any_type(), is_none_distinguishes_none_from_some(), is_some_distinguishes_some_from_none(), result_is_ok_and_is_err()

### Community 118 - "Language Server Path Setting"
Cohesion: 0.50
Nodes (4): typelisp.languageServer.path, default, markdownDescription, type

### Community 119 - "Extension Repository Metadata"
Cohesion: 0.50
Nodes (4): repository, directory, type, url

### Community 120 - "Grammar Line Comments"
Cohesion: 0.50
Nodes (4): begin, end, name, comment-line

### Community 121 - "Loop Type Checking Tests"
Cohesion: 0.50
Nodes (4): assert_type_error_with_prelude(), dotimes_count_must_be_i32(), return_inside_while_must_be_unit(), while_condition_must_be_bool_and_is_unit()

### Community 122 - "CL Equivalence Catalog"
Cohesion: 0.67
Nodes (3): Rust 実装 vs typelisp 実装の分類基準, 特殊形カタログと脱糖方針, 反復構文と break/return の意味論

### Community 123 - "Extension Runtime Dependency"
Cohesion: 0.67
Nodes (3): dependencies, vscode-languageclient, vscode-languageclient

### Community 124 - "Grammar Builtin Functions"
Cohesion: 0.67
Nodes (3): match, name, builtin-function

### Community 125 - "Grammar Char Literals"
Cohesion: 0.67
Nodes (3): match, name, char-literal

### Community 126 - "Grammar Clause Keywords"
Cohesion: 0.67
Nodes (3): match, name, clause

### Community 127 - "Grammar Constants"
Cohesion: 0.67
Nodes (3): match, name, constant

### Community 128 - "Grammar Global Variables"
Cohesion: 0.67
Nodes (3): match, name, global-variable

### Community 129 - "Grammar Keyword Symbols"
Cohesion: 0.67
Nodes (3): match, name, keyword-symbol

### Community 130 - "Grammar Lambda List"
Cohesion: 0.67
Nodes (3): match, name, lambda-list

### Community 131 - "Grammar Never Type"
Cohesion: 0.67
Nodes (3): match, name, never-type

### Community 132 - "Grammar Numbers"
Cohesion: 0.67
Nodes (3): match, name, number

### Community 133 - "Grammar Special Forms"
Cohesion: 0.67
Nodes (3): special-form, match, name

### Community 134 - "Grammar User Types"
Cohesion: 0.67
Nodes (3): user-type, match, name

## Ambiguous Edges - Review These
- `命名規則: !/? を接尾辞に使わない` → `関数・特殊形の名前に ! を一切使わない（§0.2）`  [AMBIGUOUS]
  docs/dev/cl-equivalence-catalog.md · relation: rationale_for
- `HashTable<K,V>` → `ハッシュテーブルをメソッド API として設計する提案`  [AMBIGUOUS]
  docs/dev/cl-equivalence-catalog.md · relation: conceptually_related_to

## Knowledge Gaps
- **157 isolated node(s):** `name`, `displayName`, `description`, `version`, `publisher` (+152 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **13 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **What is the exact relationship between `命名規則: !/? を接尾辞に使わない` and `関数・特殊形の名前に ! を一切使わない（§0.2）`?**
  _Edge tagged AMBIGUOUS (relation: rationale_for) - confidence is low._
- **What is the exact relationship between `HashTable<K,V>` and `ハッシュテーブルをメソッド API として設計する提案`?**
  _Edge tagged AMBIGUOUS (relation: conceptually_related_to) - confidence is low._
- **Why does `RtValue` connect `Runtime Scope Values` to `Fasl Serialization and Projects`, `Interp Compile Registry`, `Interp Numeric Builtins`, `Type Registry and Assoc Fns`, `Compile Dispatch Tests`, `Evaluator Tests`, `Compiled Closure Tests`, `Checker Module Navigation`, `LLVM Builder Builtins`, `Macro Tests`, `Heap Cons and HashTable Ops`, `Cross-Module Macro Tests`, `Pretty Printer Tests`, `Bignum and Ratio Tests`, `Format Directive Tests`, `Sequence Operation Tests`, `Defstruct Tests`, `Monomorphization Tests`, `As Conversion Tests`, `Error Trait Tests`, `Interp Parse and Read`, `Numeric Tests`, `Defenum Tests`, `Compiled Generic Bound Tests`, `Dyn Dispatch Tests`, `Scope Sharing Tests`, `HashTable Tests`, `Trait Mechanism Tests`, `Sexpr Downcast Tests`, `Closure JIT Tests`, `LLVM Module Building Tests`, `Keyword Symbol Tests`, `Dyn Match Tests`, `Generic Defun Tests`, `Comparison Operator Tests`, `Method Dispatch Tests`, `Prelude Error Tests`, `Interp Ratio Builtins`, `Island Self Compile Tests`, `Island AOT Load Tests`?**
  _High betweenness centrality (0.296) - this node is a cross-community bridge._
- **Why does `Heap` connect `GC Heap Core` to `AST to Compile Bridge`, `Format and Pretty Printer`, `Fasl Serialization and Projects`, `Interp Compile Registry`, `Interp Numeric Builtins`, `Runtime Scope Values`, `Type Registry and Assoc Fns`, `Checker Module Navigation`, `LLVM Builder Builtins`, `Source Locations and Typed AST`, `Error Types and Display`, `Runtime Shim Functions`, `Macro Tests`, `Name Lexer and Type Parsing`, `Heap Cons and HashTable Ops`, `Reader Tests`, `Generic Header Checking`, `AOT Compilation`, `Pretty Printer Tests`, `Reader Cursor`, `Defstruct Tests`, `Runtime Encode Tests`, `Error Trait Tests`, `Interp Parse and Read`, `Symbol and Path Interning`, `Runtime Enum Construction`, `Scope Sharing Tests`, `HashTable Tests`, `Dyn Box GC Tests`, `Sexpr Downcast Tests`, `Heap Allocation API`, `Generic Defun Tests`, `Cons Reference Values`, `GC Tracing Tests`, `Prelude Error Tests`, `Interp Ratio Builtins`, `Pattern Checking`, `Island Self Compile Tests`, `Sexpr Construction Tests`, `Island AOT Load Tests`?**
  _High betweenness centrality (0.155) - this node is a cross-community bridge._
- **Why does `Value` connect `Heap Cons and HashTable Ops` to `AST to Compile Bridge`, `Format and Pretty Printer`, `Fasl Serialization and Projects`, `Interp Compile Registry`, `Interp Numeric Builtins`, `Runtime Scope Values`, `Type Registry and Assoc Fns`, `Checker Module Navigation`, `LLVM Builder Builtins`, `Source Locations and Typed AST`, `Error Types and Display`, `GC Heap Core`, `Runtime Shim Functions`, `Macro Tests`, `Name Lexer and Type Parsing`, `Reader Tests`, `Generic Header Checking`, `Reader Cursor`, `Interp Parse and Read`, `Scope Frame Memory Tests`, `Defenum Tests`, `Runtime Closure GC Tests`, `Symbol and Path Interning`, `LSP Server Loop`, `Heap Allocation API`, `Cons Reference Values`, `GC Tracing Tests`, `Interp Ratio Builtins`, `Pattern Checking`, `Sexpr Construction Tests`?**
  _High betweenness centrality (0.130) - this node is a cross-community bridge._
- **What connects `name`, `displayName`, `description` to the rest of the system?**
  _157 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `AST to Compile Bridge` be split into smaller, more focused modules?**
  _Cohesion score 0.054261065871160204 - nodes in this community are weakly interconnected._