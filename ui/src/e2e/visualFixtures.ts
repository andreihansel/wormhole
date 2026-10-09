// Synthetic inventories exclusively loaded by VITE_E2E. No real endpoints or keys.
import { emptySettings, type Inventory } from "../api";
export const visualInventory: Inventory = {
  groups: [
    { id: "g1", name: "Production", parent: null, settings: emptySettings() },
    { id: "g2", name: "Edge network", parent: "g1", settings: emptySettings() },
    { id: "g3", name: "Development", parent: null, settings: emptySettings() },
  ],
  hosts: Array.from({ length: 24 }, (_, i) => ({
    id: `h${i}`,
    label:
      i === 0
        ? "edge-01 · Singapore"
        : i === 1
          ? "database-primary-with-a-very-long-descriptive-name"
          : `compute-${String(i).padStart(2, "0")}`,
    address:
      i === 1
        ? "database-primary.ap-southeast.internal.example.test"
        : `192.0.2.${10 + i}`,
    group: i < 8 ? "g2" : i < 16 ? "g1" : "g3",
    tags: i < 8 ? ["production", "nginx"] : ["linux"],
    settings: { ...emptySettings(), username: "deploy", port: 22 },
  })),
  keys: [
    {
      id: "k1",
      label: "MacBook · operator key",
      fingerprint: "SHA256:KTejSyZG3gM9GfhsHldQv1dGSyq5O50EzqU3VisualOnly",
      public_key:
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFakeSyntheticPublicKey observer@wormhole",
      certificate: null,
    },
  ],
  identities: [
    { id: "i1", label: "Production operator", username: "deploy", key: "k1" },
  ],
  snippets: [
    {
      id: "s1",
      label: "Disk usage",
      script:
        "df -h\nprintf '\\nLargest directories\\n'\ndu -sh /var/log/* | sort -h | tail -5",
    },
    {
      id: "s2",
      label: "Restart service",
      script:
        "sudo systemctl restart {{service}}\nsystemctl status {{service}} --no-pager",
    },
  ],
  forwards: [
    {
      id: "f1",
      label: "Production database",
      host: "h0",
      kind: "local",
      bind_address: "127.0.0.1",
      bind_port: 15432,
      dest_host: "localhost",
      dest_port: 5432,
    },
  ],
  has_password: ["i1"],
};
