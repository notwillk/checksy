# Generation State-Store Contract

This corpus supplies fixed vectors and filesystem scenarios for Checksy's
private generation-state substrate. Unlike the provider and pull-agent command
fixtures, the source/generation hashing, bundle digest, strict marker, atomic
snapshot, integrity-open, lease, and retention categories have focused P3
state-module coverage. The state module is not yet wired to `apply`,
`status`, enrollment, or the legacy `check` and `install` commands.

[`cases.yaml`](cases.yaml) is the machine-readable index. Text hash fields are
UTF-8; `bytesBase64Url` fields are unpadded base64url and preserve native Unix
path bytes that are not UTF-8. Every hash field uses an unsigned 64-bit
big-endian byte-length prefix. The two framing vectors prove that adjacent
fields cannot be concatenated ambiguously.

| Contract category | Executable coverage |
| --- | --- |
| Source and generation IDs | Provider tagging, length framing, strict lowercase hashes, Git object formats, and native path bytes |
| Bundle digest and integrity | Stable whole-tree hashing, strict definitions, nested file remotes, pattern confinement, bounds, and unsafe inode rejection |
| Marker and state records | Closed decoding, version handling, provider/signer matching, selection invariants, exact output, and audit/failure records |
| Store and recovery | Protected layout/modes, atomic old-or-new publication, advisory leases, orphan staging, and deterministic retention |

## Completed generation

A completed generation has this physical shape:

```text
generations/<generation-id>/
  bundle/
  generation.json
  lease
```

The closed `generation.json` object is defined by
[`state-v1.schema.json#/$defs/generationMarker`](../../../schemas/pull-agent/state-v1.schema.json#/$defs/generationMarker).
It is a content-completion receipt: the source/provider identity, configuration
path, and complete bundle digest agree, and the payload has passed local
integrity validation. It does not mean the generation converged or became
`current`.

The local provider tag has no additional fields. Git records object format and
peeled commit; HTTPS records signed generation/revision and manifest/artifact
hashes. A local diagnostic revision exists only in the selected state snapshot.

Signer, `verifiedAt`, and `promotedAt` deliberately live in the atomically
published `state.json` selection rather than this immutable marker. The same
manifest or Git commit can therefore be authenticated again under rotated
local trust without rewriting its payload identity. A trust-replacement apply
that succeeds publishes the new authentication and promotion event; an ordinary
unchanged online contact leaves the selected generation summary unchanged and
updates freshness only.

Opening a completed generation holds a shared advisory lock on `lease`, then
validates the marker, directory and recomputed generation IDs, normalized
configuration path, complete tree digest, strict configuration graph, and
confined file remotes and pattern matches. Garbage collection takes an
exclusive nonblocking lease and skips a generation while any reader holds it.

## Bundle vector

[`bundle/basic`](bundle/basic) includes a root definition, an auxiliary asset,
and one executable pattern script. Its expected digest includes every directory
and regular file beneath `bundle/`, sorted by raw normalized UTF-8 path bytes.
Ownership, timestamps, and non-executable permission bits are excluded; entry
kind, path, executable marker, file bytes, and empty directories are included.

Hashing the entire materialized payload protects assets referenced opaquely by
shell text, such as the fixture's `assets/message.txt`, but Checksy does not
parse shell commands to prove that such paths exist or remain confined. The
structured validator covers the selected config, nested file definitions, and
pattern traversal. Cross-source provider resolution and authentication remain
separate P3/P4 responsibilities and never consult the legacy `.checksy-cache`.

Unsafe inode cases are materialized by tests rather than committed. They cover
symlinks, hard links, FIFOs, sockets, changed bytes, marker/directory mismatch,
and config/pattern escape. Fault-injection cases require lock-free readers to
observe either the complete old snapshot or the complete new snapshot, never a
partial document.
