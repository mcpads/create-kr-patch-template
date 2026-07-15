//! Language-neutral patch-guard judgments exposed as MCP tools.
//!
//! Every tool maps a JSON request onto a `patch-guard` guard and returns an
//! accept/reject decision that matches the same boundaries as the conformance
//! manifests under `conformance/`. Malformed arguments are reported separately
//! as invalid parameters so a caller can tell a bad request from a rejected
//! plan.

use std::ops::Range;

use patch_guard::{
    ArtifactReport, BuildDisposition, BuildMode, ExpectedWrite, ImageRegion, LocalizationScope,
    LocalizationUnit, MachineCodeProvenance, ProductGraph, ProductStep, RegionKind,
    ReleaseApproval, ResizePlan, ReviewState, RootArtifact, RootKind, RuntimeEvidenceReport,
    RuntimeOutcome, SourceSpec, WriteIntent, WritePlan, evaluate_readiness, require_runtime_pass,
    sha256_hex, verify_exact_roundtrip, verify_source,
};
use serde_json::{Value, json};

/// A completed judgment: the guard ran and either accepted or rejected.
pub struct Judgment {
    pub accept: bool,
    pub report: Value,
}

/// Outcome of dispatching a `tools/call`.
pub enum Dispatch {
    /// The guard ran to a decision.
    Judged(Judgment),
    /// Arguments were structurally invalid; the guard never ran.
    InvalidParams(String),
    /// No tool matches the requested name.
    UnknownTool,
}

/// Result of a single tool: `Ok` once the guard decided, `Err` for bad input.
type ToolResult = Result<Judgment, String>;

/// Static tool advertisements returned by `tools/list`.
pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

/// Dispatch a `tools/call` by name.
pub fn dispatch(name: &str, args: &Value) -> Dispatch {
    let handler: fn(&Value) -> ToolResult = match name {
        "verify_source" => verify_source_tool,
        "verify_exact_roundtrip" => verify_roundtrip_tool,
        "evaluate_readiness" => evaluate_readiness_tool,
        "validate_product_graph" => validate_product_graph_tool,
        "require_runtime_pass" => require_runtime_pass_tool,
        "apply_write_plan" => apply_write_plan_tool,
        _ => return Dispatch::UnknownTool,
    };
    match handler(args) {
        Ok(judgment) => Dispatch::Judged(judgment),
        Err(message) => Dispatch::InvalidParams(message),
    }
}

fn accept(report: Value) -> Judgment {
    Judgment {
        accept: true,
        report,
    }
}

fn reject(error: &anyhow::Error) -> Judgment {
    Judgment {
        accept: false,
        report: json!({ "reason": error.to_string() }),
    }
}

fn verify_source_tool(args: &Value) -> ToolResult {
    let id = text(args, "id")?;
    let expected_len = uint(args, "expected_len")?;
    let expected_sha256 = text(args, "expected_sha256")?;
    let bytes = byte_vec(args, "bytes")?;
    let spec = SourceSpec {
        id: &id,
        len: expected_len,
        sha256: &expected_sha256,
    };
    Ok(match verify_source(spec, &bytes) {
        Ok(source) => accept(json!({
            "id": source.id,
            "len": source.len,
            "sha256": source.sha256,
        })),
        Err(error) => reject(&error),
    })
}

fn verify_roundtrip_tool(args: &Value) -> ToolResult {
    let boundary_id = text(args, "boundary_id")?;
    let original = byte_vec(args, "original")?;
    let rebuilt = byte_vec(args, "rebuilt")?;
    Ok(
        match verify_exact_roundtrip(&boundary_id, &original, &rebuilt) {
            Ok(report) => accept(to_value(&report)?),
            Err(error) => reject(&error),
        },
    )
}

fn evaluate_readiness_tool(args: &Value) -> ToolResult {
    let mode = build_mode(args, "mode")?;
    let scope_value = object(args, "scope")?;
    let scope = localization_scope(scope_value)?;
    Ok(match evaluate_readiness(mode, &scope) {
        Ok(report) => accept(to_value(&report)?),
        Err(error) => reject(&error),
    })
}

fn validate_product_graph_tool(args: &Value) -> ToolResult {
    let graph = product_graph(args)?;
    Ok(match graph.validate() {
        Ok(report) => accept(to_value(&report)?),
        Err(error) => reject(&error),
    })
}

fn require_runtime_pass_tool(args: &Value) -> ToolResult {
    let expected = artifact_report(object(args, "expected_artifact")?)?;
    let report = runtime_evidence(object(args, "report")?)?;
    Ok(match require_runtime_pass(&expected, &report) {
        Ok(()) => accept(json!({
            "scenario_id": report.scenario_id,
            "outcome": "passed",
        })),
        Err(error) => reject(&error),
    })
}

fn apply_write_plan_tool(args: &Value) -> ToolResult {
    let baseline = byte_vec(args, "baseline")?;
    let plan = write_plan(args)?;
    // No machine-code ISA verifier can cross the JSON boundary, so machine-code
    // writes always reject here, matching the `machine_code_without_verifier`
    // conformance case. A target project installs its verifier in-process.
    Ok(match plan.apply(&baseline, None) {
        Ok(result) => {
            let resize = to_value(&result.resize)?;
            let writes = to_value(&result.writes)?;
            accept(json!({
                "output": result.output,
                "output_sha256": sha256_hex(&result.output),
                "output_len": result.output.len(),
                "resize": resize,
                "writes": writes,
            }))
        }
        Err(error) => reject(&error),
    })
}

fn write_plan(args: &Value) -> Result<WritePlan, String> {
    let resize = match args.get("resize") {
        None | Some(Value::Null) => None,
        Some(value) => Some(resize_plan(value)?),
    };
    let regions = array(args, "regions")?
        .iter()
        .map(image_region)
        .collect::<Result<Vec<_>, _>>()?;
    let writes = array(args, "writes")?
        .iter()
        .map(expected_write)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(WritePlan {
        resize,
        regions,
        writes,
    })
}

fn resize_plan(value: &Value) -> Result<ResizePlan, String> {
    Ok(ResizePlan {
        actor: text(value, "actor")?,
        purpose: text(value, "purpose")?,
        expected_input_len: uint(value, "expected_input_len")?,
        output_len: uint(value, "output_len")?,
    })
}

fn image_region(value: &Value) -> Result<ImageRegion, String> {
    Ok(ImageRegion {
        id: text(value, "id")?,
        range: range(value)?,
        kind: region_kind(value, "kind")?,
        reason: text(value, "reason")?,
    })
}

fn expected_write(value: &Value) -> Result<ExpectedWrite, String> {
    Ok(ExpectedWrite {
        id: text(value, "id")?,
        actor: text(value, "actor")?,
        purpose: text(value, "purpose")?,
        offset: uint(value, "offset")?,
        expected_original: byte_vec(value, "expected_original")?,
        replacement: byte_vec(value, "replacement")?,
        intent: write_intent(value)?,
    })
}

fn write_intent(value: &Value) -> Result<WriteIntent, String> {
    match text(value, "intent")?.as_str() {
        "data" => Ok(WriteIntent::Data),
        "metadata" => Ok(WriteIntent::Metadata),
        "machine_code" => {
            let provenance = object(value, "machine_code")?;
            Ok(WriteIntent::MachineCode(MachineCodeProvenance {
                assembly_source_id: text(provenance, "assembly_source_id")?,
                isa_profile_id: text(provenance, "isa_profile_id")?,
            }))
        }
        other => Err(format!(
            "`intent` must be data, metadata, or machine_code, found `{other}`"
        )),
    }
}

fn region_kind(value: &Value, key: &str) -> Result<RegionKind, String> {
    match text(value, key)?.as_str() {
        "data" => Ok(RegionKind::Data),
        "metadata" => Ok(RegionKind::Metadata),
        "machine_code" => Ok(RegionKind::MachineCode),
        "protected" => Ok(RegionKind::Protected),
        other => Err(format!(
            "`{key}` must be data, metadata, machine_code, or protected, found `{other}`"
        )),
    }
}

fn range(value: &Value) -> Result<Range<usize>, String> {
    Ok(uint(value, "start")?..uint(value, "end")?)
}

fn localization_scope(value: &Value) -> Result<LocalizationScope, String> {
    let units = array(value, "units")?
        .iter()
        .map(localization_unit)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(LocalizationScope {
        id: text(value, "id")?,
        content_revision: text(value, "content_revision")?,
        release_approval: release_approval(value, "release_approval")?,
        approved_revision: opt_text(value, "approved_revision")?,
        units,
    })
}

fn localization_unit(value: &Value) -> Result<LocalizationUnit, String> {
    Ok(LocalizationUnit {
        id: text(value, "id")?,
        disposition: disposition(value, "disposition")?,
        review_state: review_state(value, "review_state")?,
    })
}

fn disposition(value: &Value, key: &str) -> Result<BuildDisposition, String> {
    match text(value, key)?.as_str() {
        "preserve_source" => Ok(BuildDisposition::PreserveSource),
        "use_localized" => Ok(BuildDisposition::UseLocalized),
        other => Err(format!(
            "`{key}` must be preserve_source or use_localized, found `{other}`"
        )),
    }
}

fn review_state(value: &Value, key: &str) -> Result<ReviewState, String> {
    match text(value, key)?.as_str() {
        "untranslated" => Ok(ReviewState::Untranslated),
        "draft" => Ok(ReviewState::Draft),
        "needs_review" => Ok(ReviewState::NeedsReview),
        "needs_human_review" => Ok(ReviewState::NeedsHumanReview),
        "complete" => Ok(ReviewState::Complete),
        other => Err(format!("`{key}` has unknown review state `{other}`")),
    }
}

fn release_approval(value: &Value, key: &str) -> Result<ReleaseApproval, String> {
    match text(value, key)?.as_str() {
        "pending" => Ok(ReleaseApproval::Pending),
        "approved" => Ok(ReleaseApproval::Approved),
        "rejected" => Ok(ReleaseApproval::Rejected),
        other => Err(format!(
            "`{key}` must be pending, approved, or rejected, found `{other}`"
        )),
    }
}

fn product_graph(args: &Value) -> Result<ProductGraph, String> {
    let roots = array(args, "roots")?
        .iter()
        .map(root_artifact)
        .collect::<Result<Vec<_>, _>>()?;
    let steps = array(args, "steps")?
        .iter()
        .map(product_step)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ProductGraph {
        roots,
        steps,
        final_artifacts: string_vec(args, "final_artifacts")?,
    })
}

fn root_artifact(value: &Value) -> Result<RootArtifact, String> {
    Ok(RootArtifact {
        id: text(value, "id")?,
        kind: root_kind(value, "kind")?,
    })
}

fn root_kind(value: &Value, key: &str) -> Result<RootKind, String> {
    match text(value, key)?.as_str() {
        "pure_source" => Ok(RootKind::PureSource),
        "external_derived" => Ok(RootKind::ExternalDerived),
        "research_output" => Ok(RootKind::ResearchOutput),
        other => Err(format!(
            "`{key}` must be pure_source, external_derived, or research_output, found `{other}`"
        )),
    }
}

fn product_step(value: &Value) -> Result<ProductStep, String> {
    Ok(ProductStep {
        id: text(value, "id")?,
        inputs: string_vec(value, "inputs")?,
        outputs: string_vec(value, "outputs")?,
    })
}

fn artifact_report(value: &Value) -> Result<ArtifactReport, String> {
    Ok(ArtifactReport {
        id: text(value, "id")?,
        len: uint(value, "len")?,
        sha256: text(value, "sha256")?,
    })
}

fn runtime_evidence(value: &Value) -> Result<RuntimeEvidenceReport, String> {
    let evidence = array(value, "evidence")?
        .iter()
        .map(artifact_report)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(RuntimeEvidenceReport {
        schema_version: u32::try_from(uint(value, "schema_version")?)
            .map_err(|_| "`schema_version` is too large".to_owned())?,
        scenario_id: text(value, "scenario_id")?,
        artifact: artifact_report(object(value, "artifact")?)?,
        outcome: runtime_outcome(value, "outcome")?,
        evidence,
    })
}

fn runtime_outcome(value: &Value, key: &str) -> Result<RuntimeOutcome, String> {
    match text(value, key)?.as_str() {
        "passed" => Ok(RuntimeOutcome::Passed),
        "failed" => Ok(RuntimeOutcome::Failed),
        other => Err(format!("`{key}` must be passed or failed, found `{other}`")),
    }
}

fn build_mode(args: &Value, key: &str) -> Result<BuildMode, String> {
    match text(args, key)?.as_str() {
        "development" => Ok(BuildMode::Development),
        "release_candidate" => Ok(BuildMode::ReleaseCandidate),
        other => Err(format!(
            "`{key}` must be development or release_candidate, found `{other}`"
        )),
    }
}

fn to_value<T: serde::Serialize>(value: &T) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|error| format!("serialize report: {error}"))
}

fn text(args: &Value, key: &str) -> Result<String, String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("`{key}` must be a string"))
}

fn opt_text(args: &Value, key: &str) -> Result<Option<String>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(format!("`{key}` must be a string or null")),
    }
}

fn uint(args: &Value, key: &str) -> Result<usize, String> {
    let number = args
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("`{key}` must be a non-negative integer"))?;
    usize::try_from(number).map_err(|_| format!("`{key}` is too large for this platform"))
}

fn object<'a>(args: &'a Value, key: &str) -> Result<&'a Value, String> {
    args.get(key)
        .filter(|value| value.is_object())
        .ok_or_else(|| format!("`{key}` must be an object"))
}

fn array<'a>(args: &'a Value, key: &str) -> Result<&'a [Value], String> {
    args.get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| format!("`{key}` must be an array"))
}

fn byte_vec(args: &Value, key: &str) -> Result<Vec<u8>, String> {
    array(args, key)?
        .iter()
        .map(|value| {
            let number = value
                .as_u64()
                .ok_or_else(|| format!("`{key}` contains a non-integer byte"))?;
            u8::try_from(number)
                .map_err(|_| format!("`{key}` byte {number} is out of range 0..=255"))
        })
        .collect()
}

fn string_vec(args: &Value, key: &str) -> Result<Vec<String>, String> {
    array(args, key)?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("`{key}` must contain only strings"))
        })
        .collect()
}

/// Tool advertisements. Byte arrays are declared as arrays of 0..=255 integers
/// so no external base64 dependency crosses the transport.
pub fn tool_defs() -> Vec<ToolDef> {
    let byte_array = json!({
        "type": "array",
        "items": { "type": "integer", "minimum": 0, "maximum": 255 }
    });
    let artifact = json!({
        "type": "object",
        "required": ["id", "len", "sha256"],
        "properties": {
            "id": { "type": "string" },
            "len": { "type": "integer", "minimum": 0 },
            "sha256": { "type": "string" }
        }
    });
    vec![
        ToolDef {
            name: "verify_source",
            description: "Verify supplied bytes match a declared source identity (length and SHA-256).",
            input_schema: json!({
                "type": "object",
                "required": ["id", "expected_len", "expected_sha256", "bytes"],
                "properties": {
                    "id": { "type": "string" },
                    "expected_len": { "type": "integer", "minimum": 0 },
                    "expected_sha256": { "type": "string" },
                    "bytes": byte_array
                }
            }),
        },
        ToolDef {
            name: "verify_exact_roundtrip",
            description: "Require a decode/rebuild boundary to reproduce every byte it declared.",
            input_schema: json!({
                "type": "object",
                "required": ["boundary_id", "original", "rebuilt"],
                "properties": {
                    "boundary_id": { "type": "string" },
                    "original": byte_array,
                    "rebuilt": byte_array
                }
            }),
        },
        ToolDef {
            name: "evaluate_readiness",
            description: "Judge a build mode against a localization scope; release candidates require completion and human approval.",
            input_schema: json!({
                "type": "object",
                "required": ["mode", "scope"],
                "properties": {
                    "mode": { "enum": ["development", "release_candidate"] },
                    "scope": {
                        "type": "object",
                        "required": ["id", "content_revision", "release_approval", "units"],
                        "properties": {
                            "id": { "type": "string" },
                            "content_revision": { "type": "string" },
                            "release_approval": { "enum": ["pending", "approved", "rejected"] },
                            "approved_revision": { "type": ["string", "null"] },
                            "units": {
                                "type": "array",
                                "items": {
                                    "type": "object",
                                    "required": ["id", "disposition", "review_state"],
                                    "properties": {
                                        "id": { "type": "string" },
                                        "disposition": { "enum": ["preserve_source", "use_localized"] },
                                        "review_state": { "enum": ["untranslated", "draft", "needs_review", "needs_human_review", "complete"] }
                                    }
                                }
                            }
                        }
                    }
                }
            }),
        },
        ToolDef {
            name: "validate_product_graph",
            description: "Validate that every final artifact is reproducible from pure sources through the registered product graph.",
            input_schema: json!({
                "type": "object",
                "required": ["roots", "steps", "final_artifacts"],
                "properties": {
                    "roots": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "required": ["id", "kind"],
                            "properties": {
                                "id": { "type": "string" },
                                "kind": { "enum": ["pure_source", "external_derived", "research_output"] }
                            }
                        }
                    },
                    "steps": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "required": ["id", "inputs", "outputs"],
                            "properties": {
                                "id": { "type": "string" },
                                "inputs": { "type": "array", "items": { "type": "string" } },
                                "outputs": { "type": "array", "items": { "type": "string" } }
                            }
                        }
                    },
                    "final_artifacts": { "type": "array", "items": { "type": "string" } }
                }
            }),
        },
        ToolDef {
            name: "require_runtime_pass",
            description: "Require passing runtime evidence bound to the exact build artifact hash being gated.",
            input_schema: json!({
                "type": "object",
                "required": ["expected_artifact", "report"],
                "properties": {
                    "expected_artifact": artifact,
                    "report": {
                        "type": "object",
                        "required": ["schema_version", "scenario_id", "artifact", "outcome", "evidence"],
                        "properties": {
                            "schema_version": { "type": "integer", "minimum": 0 },
                            "scenario_id": { "type": "string" },
                            "artifact": artifact,
                            "outcome": { "enum": ["passed", "failed"] },
                            "evidence": { "type": "array", "items": artifact }
                        }
                    }
                }
            }),
        },
        ToolDef {
            name: "apply_write_plan",
            description: "Apply and audit an Expected Write plan against a baseline image. Machine-code writes reject without an in-process ISA verifier.",
            input_schema: json!({
                "type": "object",
                "required": ["baseline", "regions", "writes"],
                "properties": {
                    "baseline": byte_array,
                    "resize": {
                        "type": ["object", "null"],
                        "required": ["actor", "purpose", "expected_input_len", "output_len"],
                        "properties": {
                            "actor": { "type": "string" },
                            "purpose": { "type": "string" },
                            "expected_input_len": { "type": "integer", "minimum": 0 },
                            "output_len": { "type": "integer", "minimum": 0 }
                        }
                    },
                    "regions": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "required": ["id", "start", "end", "kind", "reason"],
                            "properties": {
                                "id": { "type": "string" },
                                "start": { "type": "integer", "minimum": 0 },
                                "end": { "type": "integer", "minimum": 0 },
                                "kind": { "enum": ["data", "metadata", "machine_code", "protected"] },
                                "reason": { "type": "string" }
                            }
                        }
                    },
                    "writes": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "required": ["id", "actor", "purpose", "offset", "expected_original", "replacement", "intent"],
                            "properties": {
                                "id": { "type": "string" },
                                "actor": { "type": "string" },
                                "purpose": { "type": "string" },
                                "offset": { "type": "integer", "minimum": 0 },
                                "expected_original": byte_array,
                                "replacement": byte_array,
                                "intent": { "enum": ["data", "metadata", "machine_code"] },
                                "machine_code": {
                                    "type": "object",
                                    "required": ["assembly_source_id", "isa_profile_id"],
                                    "properties": {
                                        "assembly_source_id": { "type": "string" },
                                        "isa_profile_id": { "type": "string" }
                                    }
                                }
                            }
                        }
                    }
                }
            }),
        },
    ]
}

#[cfg(test)]
#[path = "tools_tests.rs"]
mod tests;
