// Recorder-owned exact-byte accounting. HTTP lifecycle and archive publication
// deliberately belong to callers, not this bounded local consistency protocol.
import * as fs from "node:fs";
import { createHash } from "node:crypto";

export class CaptureSession {
  constructor(output, identity, io = fs) {
    this.io = io;
    this.output = output;
    this.identity = identity;
    this.fd = io.openSync(output, "wx", 0o600);
    this.hash = createHash("sha256");
    this.bytes = 0;
    this.accepted = 0;
    this.completed = 0;
    this.failed = 0;
    this.recorded = 0;
    this.inferenceRequests = 0;
    this.pending = new Set();
    this.draining = false;
    this.poisoned = false;
    this.finalized = false;
    this.publicationAttempted = false;
    this.publicationCleanupConfirmed = false;
    this.publicationCleanupError = null;
    this.outputCleanupConfirmed = false;
    this.outputCloseError = null;
  }

  begin() {
    if (this.draining || this.finalized) return null;
    if (this.accepted >= 1_000_000) {
      this.poisoned = true;
      throw new Error("recording request count exceeds limit");
    }
    const token = { recorded: false };
    this.accepted += 1;
    this.pending.add(token);
    return token;
  }

  append(token, interaction, inference) {
    // The HTTP owner must bound both bodies before normalization/serialization.
    // This exact-byte limit is not a limit-before-allocation JSON encoder.
    this.requirePending(token);
    if (token.recorded || this.poisoned) throw new Error("recording cannot append this interaction");
    const bytes = Buffer.from(`${JSON.stringify(interaction)}\n`, "utf8");
    if (this.bytes + bytes.length > 256 * 1024 * 1024) {
      this.poisoned = true;
      throw new Error("recording payload exceeds limit");
    }
    try {
      let offset = 0;
      while (offset < bytes.length) {
        const written = this.io.writeSync(this.fd, bytes, offset, bytes.length - offset);
        if (!Number.isInteger(written) || written <= 0 || written > bytes.length - offset) {
          throw new Error("recording made invalid write progress");
        }
        this.hash.update(bytes.subarray(offset, offset + written));
        this.bytes += written;
        offset += written;
      }
      token.recorded = true;
      this.recorded += 1;
      if (inference) this.inferenceRequests += 1;
    } catch (error) {
      this.poisoned = true;
      throw error;
    }
  }

  end(token, delivered) {
    this.requirePending(token);
    this.pending.delete(token);
    if (token.recorded && delivered) this.completed += 1;
    else this.failed += 1;
  }

  drain() {
    this.draining = true;
  }

  requirePending(token) {
    if (this.fd === null || !this.pending.has(token)) {
      throw new Error("recording handler is not owned or already settled");
    }
  }

  finalize(receipt) {
    if (this.publicationAttempted) throw new Error("recording finalization already attempted");
    this.drain();
    if (this.pending.size !== 0) throw new Error("recording handlers have not quiesced");
    this.publicationAttempted = true;
    // A failed sync/close can never publish positive finalization evidence.
    this.io.fsyncSync(this.fd);
    this.closeOutput();
    const value = {
      schema_version: 1,
      mode: "record",
      pid: this.identity.pid,
      nonce: this.identity.nonce,
      output: this.output,
      accepted: this.accepted,
      completed: this.completed,
      failed: this.failed,
      inflight: 0,
      recorded: this.recorded,
      inference_requests: this.inferenceRequests,
      bytes: this.bytes,
      sha256: this.hash.copy().digest("hex"),
      complete: !this.poisoned && this.failed === 0 && this.completed === this.recorded,
    };
    const temporary = `${receipt}.${this.identity.pid}.tmp`;
    const fd = this.io.openSync(temporary, "wx", 0o600);
    try {
      this.io.writeFileSync(fd, JSON.stringify(value));
      this.io.fsyncSync(fd);
    } finally {
      this.io.closeSync(fd);
    }
    // Atomic no-clobber publication, including a racing destination.
    this.io.linkSync(temporary, receipt);
    this.finalized = true;
    // Receipt publication is authoritative even if removing its owned temporary
    // hard link fails. Callers must report cleanup separately, never republish.
    try {
      this.io.unlinkSync(temporary);
      this.publicationCleanupConfirmed = true;
    } catch (error) {
      this.publicationCleanupError = error;
    }
    return value;
  }

  close() {
    this.drain();
    this.closeOutput();
  }

  closeOutput() {
    if (this.outputCloseError !== null) throw this.outputCloseError;
    if (this.fd !== null) {
      const fd = this.fd;
      this.fd = null;
      try {
        this.io.closeSync(fd);
        this.outputCleanupConfirmed = true;
      } catch (error) {
        this.outputCloseError = error;
        throw error;
      }
    }
  }
}
