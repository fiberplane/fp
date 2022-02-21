#!/bin/bash

# strict mode
set -euo pipefail
IFS=$'\n\t'

# Custom variables
version="${1?"please specify a version"}"

# Fixed vars
arch="linux_x86_64" # TODO: Change value to macos!
binary_location="/tmp/fp"
cloud_front_distribution_id="E31H3YWL4HYDSY"
manifest_location="/tmp/fp-${arch}-manifest.json"
versioned_manifest_url="fp.dev/fp/${version}/${arch}/manifest.json"
latest_manifest_url="fp.dev/fp/latest/${arch}/manifest.json"

>&2 echo "--> Creating release for version: ${version}; host triple: ${arch}"

# Start the download
curl "https://fp.dev/fp/${version}/${arch}/fp" \
    --fail \
    --progress-bar \
    -o "${binary_location}"
>&2 echo "--> Download complete (${binary_location})"

# Make executable and dump manifest
chmod u+x "${binary_location}"
"${binary_location}" version \
    -o json \
    --disable-version-check \
    > "${manifest_location}"
>&2 echo "--> Manifest dump successful (${manifest_location})"

# Upload manifest to GitHub release
gh release upload "${version}" "${manifest_location}" --repo fiberplane/fp
>&2 echo "--> GitHub release updated with manifest ()"

# Upload manifest to S3 bucket (versionend)
aws s3 cp \
    --acl public-read \
    "${manifest_location}" \
    "s3://${versioned_manifest_url}"
>&2 echo "--> Upload to S3 bucket (versioned) completed (${versioned_manifest_url})"

>&2 echo -n "Update latest manifest on S3? [Y/n]: "
read -r update_latest
if [ "${update_latest}" == "n" ]; then
    >&2 echo "--> Skipping update for latest"
else
    # Upload manifest to S3 bucket (latest)
    aws s3 cp \
        --acl public-read \
        "${manifest_location}" \
        "s3://${latest_manifest_url}"
    >&2 echo "--> Upload to S3 bucket (latest) completed (${latest_manifest_url})"

    # Invalidate cloudfront
    aws cloudfront create-invalidation \
        --distribution-id "${cloud_front_distribution_id}" \
        --paths "${latest_manifest_url}"
    >&2 echo "--> CloudFront invalidation completed (${cloud_front_distribution_id})"
fi

>&2 echo "--> Finished uploading manifest files for version: ${version}; host triple: ${arch}"
