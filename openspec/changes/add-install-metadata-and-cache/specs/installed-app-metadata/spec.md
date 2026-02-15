## ADDED Requirements

### Requirement: Enhanced metadata in installed program listing
The system SHALL return enhanced metadata for each installed program, including icon path, install date, size value, size source, and metadata confidence, while preserving backward-compatible existing fields.

#### Scenario: Registry entry contains all metadata fields
- **WHEN** the listing pipeline reads a registry uninstall entry with `DisplayIcon`, `InstallDate`, and `EstimatedSize`
- **THEN** the returned program record includes non-null icon path, normalized install date, and size value with explicit source metadata

### Requirement: Install date normalization
The system SHALL normalize install date values into `YYYY-MM-DD` format when source data is parseable and MUST return null with low confidence when source data is invalid or ambiguous.

#### Scenario: Unparseable install date
- **WHEN** a registry entry contains an invalid install date string
- **THEN** the returned program record has null install date and metadata confidence marked as low for the date field

### Requirement: Icon path sanitization and validation
The system MUST sanitize icon references from source strings and SHALL only return icon paths that resolve to existing local files.

#### Scenario: DisplayIcon includes parameters
- **WHEN** `DisplayIcon` contains a path with icon index or command parameters
- **THEN** the system strips non-path segments, validates the file existence, and returns a clean path or null if validation fails

### Requirement: Conservative size resolution
The system SHALL prioritize registry `EstimatedSize` as size input and MAY use filesystem fallback calculation only when estimated size is missing and install location is trustworthy.

#### Scenario: Estimated size missing with valid install location
- **WHEN** a program has no `EstimatedSize` but has a valid install location
- **THEN** the system attempts bounded filesystem size calculation and records the result source as filesystem or unknown on timeout/failure

