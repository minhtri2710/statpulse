import { Action, ActionPanel, Icon, List, open } from "@raycast/api";

const OPTIONS = [
  { id: "30m", title: "Keep Awake for 30 Minutes", subtitle: "Prevent idle sleep for 30m" },
  { id: "1h", title: "Keep Awake for 1 Hour", subtitle: "Prevent idle sleep for 1h" },
  { id: "2h", title: "Keep Awake for 2 Hours", subtitle: "Prevent idle sleep for 2h" },
  { id: "always", title: "Keep Awake Indefinitely", subtitle: "Prevent idle sleep until turned off" },
  { id: "off", title: "Disable Keep Awake", subtitle: "Allow normal macOS sleep" },
];

export default function Command() {
  return (
    <List searchBarPlaceholder="Select Keep Awake duration...">
      {OPTIONS.map((opt) => (
        <List.Item
          key={opt.id}
          icon={opt.id === "off" ? Icon.Moon : Icon.Sun}
          title={opt.title}
          subtitle={opt.subtitle}
          actions={
            <ActionPanel>
              <Action title="Apply" onAction={() => open(`statpulse://awake/${opt.id}`)} />
            </ActionPanel>
          }
        />
      ))}
    </List>
  );
}
