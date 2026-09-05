#!/usr/bin/env bash
# Use a fresh container filesystem; never attach production data.
set -euo pipefail
image=${1:?usage: smoke-image.sh IMAGE}
container=$(docker run --detach --publish 127.0.0.1::3000 "$image")
trap 'docker rm --force --volumes "$container" >/dev/null' EXIT
base="http://$(docker port "$container" 3000/tcp)"

# This service has no dedicated health route. Its read-only favorites endpoint
# checks that both the HTTP server and the initialized database are available.
for attempt in {1..40}; do
  if curl --fail --silent "$base/api/favorites" >/dev/null; then
    break
  fi
  if [[ "$attempt" == 40 ]]; then
    docker logs "$container"
    exit 1
  fi
  sleep 0.5
done

test "$(curl --fail --silent "$base/api/favorites")" = '[]'
curl --fail --silent "$base/" | grep --ignore-case '<!doctype html'
test "$(curl --fail --silent "$base/reader/smoke")" = "$(curl --fail --silent "$base/")"
docker exec "$container" sh -c '
  for binary in viewer-of-5ch migrate-image-cache resize-image-cache; do
    test -x "/usr/local/bin/$binary" || exit 1
    if ldd "/usr/local/bin/$binary" | grep -q "not found"; then exit 1; fi
  done
'
