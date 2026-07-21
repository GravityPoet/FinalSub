import assert from "node:assert/strict";
import test from "node:test";

import { windowsSigningConfigFromEnvironment } from "./windows-signing-config.mjs";

test("omits Windows signing config when it is optional and absent", () => {
  assert.equal(
    windowsSigningConfigFromEnvironment({ environment: {} }),
    null,
  );
});

test("normalizes a complete Windows signing config", () => {
  const config = windowsSigningConfigFromEnvironment({
    required: true,
    environment: {
      WINDOWS_CERTIFICATE_THUMBPRINT:
        "0123456789abcdef0123456789abcdef01234567",
      WINDOWS_TIMESTAMP_URL: "http://timestamp.digicert.com",
    },
  });

  assert.deepEqual(config, {
    certificateThumbprint: "0123456789ABCDEF0123456789ABCDEF01234567",
    digestAlgorithm: "sha256",
    timestampUrl: "http://timestamp.digicert.com/",
    tsp: true,
  });
});

test("fails closed when the timestamp URL is missing", () => {
  assert.throws(
    () =>
      windowsSigningConfigFromEnvironment({
        required: true,
        environment: {
          WINDOWS_CERTIFICATE_THUMBPRINT:
            "0123456789ABCDEF0123456789ABCDEF01234567",
        },
      }),
    /WINDOWS_TIMESTAMP_URL/,
  );
});

test("rejects timestamp URLs outside HTTP or HTTPS", () => {
  assert.throws(
    () =>
      windowsSigningConfigFromEnvironment({
        required: true,
        environment: {
          WINDOWS_CERTIFICATE_THUMBPRINT:
            "0123456789ABCDEF0123456789ABCDEF01234567",
          WINDOWS_TIMESTAMP_URL: "file:///tmp/timestamp",
        },
      }),
    /HTTP or HTTPS/,
  );
});
