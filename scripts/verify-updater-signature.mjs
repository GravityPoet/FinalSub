import { createHash, createPublicKey, verify } from "node:crypto";
import { readFile } from "node:fs/promises";

const MINISIGN_PUBLIC_KEY_BYTES = 42;
const MINISIGN_SIGNATURE_BYTES = 74;
const ED25519_SPKI_PREFIX = Buffer.from("302a300506032b6570032100", "hex");

function decodeOuterBase64(value, label) {
  const decoded = Buffer.from(value.trim(), "base64").toString("utf8");
  if (!decoded) {
    throw new Error(`${label} is empty or not valid base64`);
  }
  return decoded;
}

export function verifyUpdaterSignature({ artifact, encodedPublicKey, encodedSignature }) {
  const publicKeyLines = decodeOuterBase64(encodedPublicKey, "updater public key")
    .trim()
    .split(/\r?\n/);
  if (publicKeyLines.length !== 2) {
    throw new Error("updater public key has an invalid Minisign structure");
  }
  const publicKeyBytes = Buffer.from(publicKeyLines[1], "base64");
  if (publicKeyBytes.length !== MINISIGN_PUBLIC_KEY_BYTES) {
    throw new Error("updater public key has an invalid Minisign length");
  }

  const signatureLines = decodeOuterBase64(encodedSignature, "updater signature")
    .trim()
    .split(/\r?\n/);
  if (
    signatureLines.length !== 4 ||
    !signatureLines[2].startsWith("trusted comment: ")
  ) {
    throw new Error("updater signature has an invalid Minisign structure");
  }
  const signatureBytes = Buffer.from(signatureLines[1], "base64");
  const globalSignature = Buffer.from(signatureLines[3], "base64");
  if (signatureBytes.length !== MINISIGN_SIGNATURE_BYTES || globalSignature.length !== 64) {
    throw new Error("updater signature has an invalid Minisign length");
  }
  if (!signatureBytes.subarray(0, 2).equals(Buffer.from("ED"))) {
    throw new Error("updater signature must use Minisign prehashed mode");
  }
  if (!publicKeyBytes.subarray(2, 10).equals(signatureBytes.subarray(2, 10))) {
    throw new Error("updater signature was created by a different key");
  }

  const publicKey = createPublicKey({
    key: Buffer.concat([ED25519_SPKI_PREFIX, publicKeyBytes.subarray(10)]),
    format: "der",
    type: "spki",
  });
  const artifactHash = createHash("blake2b512").update(artifact).digest();
  if (!verify(null, artifactHash, publicKey, signatureBytes.subarray(10))) {
    throw new Error("updater artifact signature verification failed");
  }

  const trustedComment = Buffer.from(
    signatureLines[2].slice("trusted comment: ".length),
    "utf8",
  );
  const globalPayload = Buffer.concat([
    signatureBytes.subarray(10),
    trustedComment,
  ]);
  if (!verify(null, globalPayload, publicKey, globalSignature)) {
    throw new Error("updater signature trusted-comment verification failed");
  }
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const [artifactPath, signaturePath, publicKeyPath] = process.argv.slice(2);
  if (!artifactPath || !signaturePath || !publicKeyPath) {
    console.error(
      "Usage: node scripts/verify-updater-signature.mjs <artifact> <signature> <public-key>",
    );
    process.exit(2);
  }
  try {
    const [artifact, encodedSignature, encodedPublicKey] = await Promise.all([
      readFile(artifactPath),
      readFile(signaturePath, "utf8"),
      readFile(publicKeyPath, "utf8"),
    ]);
    verifyUpdaterSignature({ artifact, encodedPublicKey, encodedSignature });
    console.log("UPDATER_SIGNATURE_OK");
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}
