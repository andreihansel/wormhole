import { useMemo, useRef, useState } from "react";
import * as Popover from "@radix-ui/react-popover";
import {
  ChevronRight,
  Folder,
  FolderOpen,
  ListFilter,
  Plus,
  Search,
  Server,
  Pencil,
} from "lucide-react";
import { api, errorMessage, type Group, type Host, type Id } from "../api";
import { Button, EmptyState, PageHeader, StatusBadge } from "../components/ui";
import { groupPath } from "../inherit";
import { useStore } from "../store";
import HostForm from "./HostForm";
function matches(h: Host, query: string) {
  const hay = [h.label, h.address, ...h.tags].map((s) => s.toLowerCase());
  return query
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .every((term) => hay.some((s) => s.includes(term)));
}
function GroupTree({
  groups,
  parent,
  selected,
  onSelect,
}: {
  groups: Group[];
  parent: Id | null;
  selected: Id | null;
  onSelect: (id: Id) => void;
}) {
  const children = groups
    .filter((g) => g.parent === parent)
    .sort((a, b) => a.name.localeCompare(b.name));
  return children.length ? (
    <>
      {children.map((g) => (
        <li
          key={g.id}
          role="treeitem"
          aria-label={g.name}
          aria-selected={selected === g.id}
        >
          <button
            className={selected === g.id ? "tree-btn active" : "tree-btn"}
            onClick={() => onSelect(g.id)}
          >
            <ChevronRight size={13} aria-hidden="true" />
            <Folder size={14} aria-hidden="true" />
            {g.name}
          </button>
          {groups.some((c) => c.parent === g.id) && (
            <ul role="group">
              <GroupTree
                groups={groups}
                parent={g.id}
                selected={selected}
                onSelect={onSelect}
              />
            </ul>
          )}
        </li>
      ))}
    </>
  ) : null;
}
export default function HostsView() {
  const { inventory, openTab, tabs } = useStore();
  const [query, setQuery] = useState("");
  const [selectedGroup, setSelectedGroup] = useState<Id | null>(null);
  const [editing, setEditing] = useState<Host | "new" | null>(null);
  const [errors, setErrors] = useState<Record<Id, string>>({});
  const [connecting, setConnecting] = useState<Set<Id>>(new Set());
  const inFlight = useRef(new Set<Id>());
  const [groupsOpen, setGroupsOpen] = useState(false);
  const visible = useMemo(
    () =>
      inventory.hosts
        .filter(
          (h) =>
            !selectedGroup ||
            groupPath(inventory.groups, h.group).some(
              (g) => g.id === selectedGroup,
            ),
        )
        .filter((h) => matches(h, query))
        .sort((a, b) => a.label.localeCompare(b.label)),
    [inventory, query, selectedGroup],
  );
  async function connect(h: Host) {
    if (inFlight.current.has(h.id)) return;
    inFlight.current.add(h.id);
    setConnecting(new Set(inFlight.current));
    setErrors(({ [h.id]: _, ...rest }) => rest);
    try {
      const session = await api.openSshSession(h.id, 80, 24);
      openTab({ session, title: h.label, host: h.id });
    } catch (e) {
      setErrors((prev) => ({ ...prev, [h.id]: errorMessage(e) }));
    } finally {
      inFlight.current.delete(h.id);
      setConnecting(new Set(inFlight.current));
    }
  }
  const groupName = (id: Id | null) =>
    inventory.groups.find((g) => g.id === id)?.name;
  const select = (id: Id) => {
    setSelectedGroup(id);
    setGroupsOpen(false);
  };
  const groups = (
    <>
      <button
        className={selectedGroup === null ? "tree-btn active" : "tree-btn"}
        onClick={() => {
          setSelectedGroup(null);
          setGroupsOpen(false);
        }}
      >
        <FolderOpen size={14} aria-hidden="true" />
        All hosts<span className="count">{inventory.hosts.length}</span>
      </button>
      <ul role="tree" aria-label="Groups" className="tree">
        <GroupTree
          groups={inventory.groups}
          parent={null}
          selected={selectedGroup}
          onSelect={select}
        />
      </ul>
    </>
  );
  return (
    <div className="split-view">
      <nav className="subnav" aria-label="Host groups">
        <h3 className="subnav-title">Your inventory</h3>
        {groups}
        {inventory.groups.length === 0 && (
          <p className="muted small" style={{ padding: "16px 10px" }}>
            Organize connections with groups from your vault.
          </p>
        )}
      </nav>
      <section className="content">
        <PageHeader
          title={
            selectedGroup ? (groupName(selectedGroup) ?? "Hosts") : "Hosts"
          }
          count={visible.length}
          description="Your servers. One secure place to reach them."
          actions={
            <Button
              className="primary"
              icon={Plus}
              onClick={() => setEditing("new")}
            >
              New host
            </Button>
          }
        />
        <div className="toolbar host-toolbar">
          <div className="search-wrap">
            <Search size={16} aria-hidden="true" />
            <input
              type="search"
              aria-label="Search hosts"
              placeholder="Search by name, address or tag"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
          </div>
          <Popover.Root open={groupsOpen} onOpenChange={setGroupsOpen}>
            <Popover.Trigger asChild>
              <Button className="compact-groups" icon={ListFilter}>
                Groups
              </Button>
            </Popover.Trigger>
            <Popover.Portal>
              <Popover.Content
                className="group-popover"
                sideOffset={8}
                align="end"
                aria-label="Filter host groups"
              >
                {groups}
              </Popover.Content>
            </Popover.Portal>
          </Popover.Root>
        </div>
        {visible.length === 0 && (
          <EmptyState
            icon={Server}
            title={
              inventory.hosts.length === 0
                ? "Make your first connection."
                : "No hosts match."
            }
          >
            {inventory.hosts.length === 0
              ? "Add a host, or import your existing SSH config from Settings. Your connections stay on this device."
              : "Try a different name, address, tag, or group."}
          </EmptyState>
        )}
        <div className="host-list">
          {visible.map((h) => (
            <article key={h.id} className="host-row">
              <Server
                className="host-icon"
                size={22}
                strokeWidth={1.4}
                aria-hidden="true"
              />
              <div className="host-info" onDoubleClick={() => connect(h)}>
                <h3>{h.label}</h3>
                <p className="mono muted">
                  {h.settings.username ? `${h.settings.username}@` : ""}
                  {h.address}
                  {h.settings.port ? `:${h.settings.port}` : ""}
                </p>
                <div className="row">
                  <span className="small muted">{groupName(h.group)}</span>
                  {tabs.some((t) => t.host === h.id && !t.closed) && (
                    <StatusBadge state="live">Session open</StatusBadge>
                  )}
                </div>
              </div>
              <div className="tags">
                {h.tags.map((t) => (
                  <span className="tag" key={t}>
                    {t}
                  </span>
                ))}
              </div>
              <div className="host-actions">
                <Button
                  className="ghost"
                  icon={Pencil}
                  aria-label={`Edit ${h.label}`}
                  onClick={() => setEditing(h)}
                >
                  Edit
                </Button>
                <Button
                  className="primary"
                  aria-label={`Connect to ${h.label}`}
                  busy={connecting.has(h.id)}
                  onClick={() => connect(h)}
                >
                  {connecting.has(h.id) ? "Connecting…" : "Connect"}
                </Button>
              </div>
              {errors[h.id] && (
                <p role="alert" className="error small">
                  {errors[h.id]}
                </p>
              )}
            </article>
          ))}
        </div>
      </section>
      {editing && (
        <HostForm
          host={editing === "new" ? null : editing}
          defaultGroup={selectedGroup}
          onClose={() => setEditing(null)}
        />
      )}
    </div>
  );
}
