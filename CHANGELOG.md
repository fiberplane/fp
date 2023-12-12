# Changelog

All notable changes to this project will be documented in this file.

The format of this file is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

## [2.16.0]

- Update all dependencies, except `hyper` (#260)
- Allow overriding the token through an argument or envvar (#261)
- Replace Lint code in our CI with Clippy+reviewdog (#262)

## [2.11.0]

### Added

- Added builds for Linux ARM64
- Added support for "Sign in with GitHub" (#248)

## [2.9.0]

### Added

- New arguments for `fp views update` (#233):
  - `--clear-description`: Removes existing description from view
  - `--clear-time-range`: Removes existing time range from view
  - `--clear-sort-by`: Removes existing sort by from view
  - `--clear-sort-direction`: Removes existing sort direction from view
- Add new command `fp webhooks` used to interact with webhooks and their deliveries
- `fp webhooks create` now takes a `enabled` parameter (#242)
- Webhooks commands now output whenever the latest delivery was successful (#242)

### Changed

- Suggest using homebrew to update fp if we think that fp is installed through homebrew (#230)

## [2.8.0]

### Added

- Add more aliases to DataSources command (#222)
- Added ability to sort the output of the notebook search command (#232)

### Changed

- Rename Event in the providers module to ProviderEvent (#231)

### Fixed

- Fix publishing docs to ReadMe (#229)
- Fix being unable to clear optional fields on `fp views` (#233)

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
