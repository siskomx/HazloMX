import type { Phase, Snapshot } from "./contracts";

// Rust parses this same JSON literal; tsc checks it against the types used by the interface.
export const contractFixture = (
// JSON-BEGIN
{
  "dimension_limits": { "minimum": 1.0, "maximum": 200.0 },
  "phases": { "awaiting_approval": true, "running": true, "cancelled": true, "failed": true, "interrupted": true, "completed": true },
  "snapshot": {
    "schema_version": 2,
    "project_id": "96857804-4d3f-4e8e-8fab-7b4331a88d0b",
    "input": {
      "schema_version": 1,
      "job_id": "c00b7330-d78d-4903-a483-302044811c38",
      "dimensions_mm": { "length": 12.5, "width": 7.25, "height": 3.5 }
    },
    "phase": "awaiting_approval",
    "audit": {
      "source": { "provider": "ollama", "model": "kimi-k2.7-code:cloud", "digest": "eda07a6592375dcbde7cf167b6d6b368cdd28e244f9d71559fb59919aca882fa", "cloud": true, "runtime_version": "0.35.1" },
      "user_edited": false,
      "files_verified": false,
      "adapter_version": 1,
      "destination": ".hazlomexa/artifacts/c00b7330-d78d-4903-a483-302044811c38",
      "created_unix_ms": 1791706152676,
      "updated_unix_ms": 1791706152676,
      "sequence": 1,
      "error": null
    }
  },
  "agent_snapshot": {
    "schema_version": 4,
    "project_id": "96857804-4d3f-4e8e-8fab-7b4331a88d0b",
    "input": { "schema_version": 1, "job_id": "9939ec51-16c1-4d0b-8027-57fbc76350b1", "agent_version": "0.7.1", "source_name": "Original.ino", "original_source": "void setup() {} void loop() {}", "source": "void setup() {} void loop() { delay(500); }" },
    "phase": "awaiting_approval",
    "audit": { "source": { "provider": "manual" }, "user_edited": false, "files_verified": false, "adapter_version": 1, "destination": ".hazlomexa/artifacts/9939ec51-16c1-4d0b-8027-57fbc76350b1", "created_unix_ms": 1791757882000, "updated_unix_ms": 1791757882000, "sequence": 1, "error": null }
  },
  "drawing_snapshot": {
    "schema_version": 5,
    "project_id": "96857804-4d3f-4e8e-8fab-7b4331a88d0b",
    "input": { "schema_version": 1, "job_id": "a1980b09-2a7b-48dd-9d8c-9ef188a79f9e", "cadcraft_version": "0.5.0", "source_name": "Original.dxf", "source_format": "dxf", "source_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "output_format": "dwg", "units": "in", "circle": {"center":[2.0,0.0],"radius":0.5}, "allow_loss": false, "original_summary": {"entity_count":1,"counts":{"Circle":1},"extents":[[-10.0,-10.0],[10.0,10.0]]} },
    "phase": "awaiting_approval",
    "audit": { "source": { "provider": "manual" }, "user_edited": false, "files_verified": false, "adapter_version": 1, "destination": ".hazlomexa/artifacts/a1980b09-2a7b-48dd-9d8c-9ef188a79f9e", "created_unix_ms": 1791757882000, "updated_unix_ms": 1791757882000, "sequence": 1, "error": null }
  },
  "compile_snapshot": {
    "schema_version": 3,
    "project_id": "96857804-4d3f-4e8e-8fab-7b4331a88d0b",
    "input": { "schema_version": 1, "job_id": "e064d059-028b-4cf9-bd27-24a892304d6e", "fqbn": "arduino:avr:uno", "source": "void setup() {} void loop() {}" },
    "phase": "awaiting_approval",
    "audit": {
      "source": { "provider": "manual" }, "user_edited": false, "files_verified": false,
      "adapter_version": 1, "destination": ".hazlomexa/artifacts/e064d059-028b-4cf9-bd27-24a892304d6e",
      "created_unix_ms": 1791757882000, "updated_unix_ms": 1791757882000,
      "sequence": 1, "error": null
    }
  }
}
// JSON-END
) satisfies { dimension_limits: { minimum: number; maximum: number }; phases: Record<Phase, boolean>; snapshot: Snapshot; compile_snapshot: Snapshot; agent_snapshot: Snapshot; drawing_snapshot: Snapshot };
