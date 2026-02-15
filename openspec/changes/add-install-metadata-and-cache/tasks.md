## 1. Data Model and Contract

- [x] 1.1 Extend installed program model with metadata source/confidence fields while keeping existing fields backward-compatible
- [x] 1.2 Define shared enums/values for size source, date source, and confidence levels used by command responses
- [x] 1.3 Update Tauri command parameter/response types to include cache control flag and enhanced metadata fields

## 2. Metadata Enrichment Pipeline

- [x] 2.1 Implement install date normalization to `YYYY-MM-DD` with invalid-data fallback to null and low confidence
- [x] 2.2 Implement display icon sanitization and file-existence validation with conservative null fallback
- [x] 2.3 Implement bounded size resolution flow (EstimatedSize first, filesystem fallback with timeout/concurrency limits)
- [x] 2.4 Integrate enrichment stage into listing flow without blocking UI thread

## 3. Cache Layer

- [x] 3.1 Introduce versioned cache file structure with generation timestamp and entry list
- [x] 3.2 Implement read-through cache logic with validity checks (schema version + TTL)
- [x] 3.3 Implement force-refresh path that bypasses cache and rewrites cache with fresh scan results
- [x] 3.4 Add cache invalidation hook after successful uninstall mutation

## 4. Frontend Integration

- [x] 4.1 Update frontend installed-app types to align with enhanced backend metadata and cache state fields
- [x] 4.2 Render icon/install date/size with explicit fallback and low-confidence indicators in app list UI
- [x] 4.3 Add refresh entry point in UI to request forced rescan when user needs latest data

## 5. Verification and Quality Gates

- [x] 5.1 Add unit tests for date normalization, icon path sanitization, and size-source selection logic
- [x] 5.2 Add tests for cache hit, cache miss, schema-version mismatch, and force-refresh behaviors
- [x] 5.3 Run project checks and ensure no regressions in existing list/scan/uninstall command behavior
