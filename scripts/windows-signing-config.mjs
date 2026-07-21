const thumbprintPattern = /^[A-F0-9]{40}$/;

export function windowsSigningConfigFromEnvironment({
  required = false,
  environment = process.env,
} = {}) {
  const thumbprint = (
    environment.WINDOWS_CERTIFICATE_THUMBPRINT ?? ""
  )
    .replace(/\s/g, "")
    .toUpperCase();
  const timestampValue = (environment.WINDOWS_TIMESTAMP_URL ?? "").trim();
  const hasAnySigningValue = thumbprint.length > 0 || timestampValue.length > 0;

  if (!required && !hasAnySigningValue) {
    return null;
  }
  if (!thumbprintPattern.test(thumbprint)) {
    throw new Error(
      "WINDOWS_CERTIFICATE_THUMBPRINT must be the 40-character SHA-1 certificate thumbprint",
    );
  }

  let timestampUrl;
  try {
    timestampUrl = new URL(timestampValue);
  } catch {
    throw new Error("WINDOWS_TIMESTAMP_URL must be an absolute URL");
  }
  if (!["http:", "https:"].includes(timestampUrl.protocol)) {
    throw new Error("WINDOWS_TIMESTAMP_URL must use HTTP or HTTPS");
  }
  if (timestampUrl.username || timestampUrl.password || timestampUrl.hash) {
    throw new Error(
      "WINDOWS_TIMESTAMP_URL must not contain credentials or a fragment",
    );
  }

  return {
    certificateThumbprint: thumbprint,
    digestAlgorithm: "sha256",
    timestampUrl: timestampUrl.toString(),
    tsp: true,
  };
}
