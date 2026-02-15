## ADDED Requirements

### Requirement: Read-through cache for installed program scans
The system SHALL use a local read-through cache for installed program scan results and MUST return cached data when the cache is valid and no force-refresh is requested.

#### Scenario: Valid cache hit
- **WHEN** a listing request is received and cache data is present within validity constraints
- **THEN** the system returns cached program records without triggering a full rescan

### Requirement: Explicit force refresh behavior
The system SHALL support a force-refresh flag that bypasses cache reads and MUST rebuild cache content from fresh scan results in the same request flow.

#### Scenario: Force refresh requested
- **WHEN** a listing request is sent with refresh enabled
- **THEN** the system performs a fresh scan, writes new cache content, and returns the refreshed records

### Requirement: Cache metadata and versioning
The cache storage MUST include schema version and generation timestamp so that incompatible or outdated cache data can be detected deterministically.

#### Scenario: Cache schema version mismatch
- **WHEN** stored cache schema version differs from the current runtime schema version
- **THEN** the system treats the cache as invalid and rebuilds it before returning results

### Requirement: Cache invalidation after uninstall mutation
The system MUST invalidate affected cache entries or the full cache after successful uninstall operations to prevent stale listing data.

#### Scenario: Program uninstalled successfully
- **WHEN** an uninstall command reports success for a program present in cached listing data
- **THEN** the next listing request triggers cache invalidation handling and does not return stale program presence

