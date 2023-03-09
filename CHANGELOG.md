# Changelog

All notable changes to this project will be documented in this file.

The format of this file is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Added

- Add more aliases to DataSources command (#222)
- Added ability to sort the output of the notebook search command (#232)

### Changed

- Rename Event in the providers module to ProviderEvent (#231)

### Fixed

- Fix publishing docs to ReadMe (#229)

## [2.7.0]

### Fixed

- Fix interactive notebook picker displaying the oldest notebooks first (#220)

### Added

- Display description in data source table view (#220)
- Add message informing user how to exit `fp shell` (#220)

### Removed

- Removed support for the legacy provider protocol. The `provider invoke`
  command now uses the new protocol (`invoke2` still exists as an alias).

## [2.6.0]

### Added

- Initial open-source release of `fp`.

### Fixed

- [CI] Fixed syncing of the `fp` reference to ReadMe (#213)
- Update some crates to resolve some security vulnerabilities (#212)
- Update dependencies to resolve dependabot issues (#215)

### Added

- Added support for views v2 properties `color`, `time range` and `sorting` (#218)

## [2.5.1]

### Fixed

- [CI] Fixed syncing of the `fp` reference (#210)

## 2.5.0

### Fixed

- Resolved clippy warnings (#208)
