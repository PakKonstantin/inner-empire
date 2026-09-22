/**
 * The design system.
 *
 * One import site for every primitive the interface is built from, including
 * the four that predate this layer — the modal, the context menu, the toasts
 * and the virtual list. Re-exporting them here rather than moving them keeps
 * their history and their callers intact while making `@/ui` the one place a
 * component comes from.
 */

export { Button, ButtonGroup, IconButton } from './Button';
export type { ButtonProps, ButtonSize, ButtonVariant, IconButtonProps } from './Button';

export { Checkbox, Input, SearchInput, Select, Toggle } from './Input';
export type {
  CheckboxProps,
  InputProps,
  SearchInputProps,
  SelectOption,
  SelectProps,
  ToggleProps,
} from './Input';

export {
  Badge,
  EmptyState,
  ErrorState,
  FilterChip,
  LoadingState,
  Skeleton,
  TagChip,
} from './Feedback';
export type {
  BadgeTone,
  EmptyStateProps,
  ErrorStateProps,
  FilterChipProps,
  LoadingStateProps,
  SkeletonProps,
  TagChipProps,
} from './Feedback';

export { CollapsibleSection, Panel, PanelStack } from './Panel';
export type { CollapsibleSectionProps, PanelProps } from './Panel';

export { Breadcrumb, Rail, SegmentedControl, useRovingFocus } from './Navigation';
export type {
  BreadcrumbSegment,
  RailItem,
  RailProps,
  SegmentedControlProps,
  SegmentedOption,
} from './Navigation';

export { Tooltip } from './Tooltip';
export type { TooltipProps } from './Tooltip';

export { menu, MenuButton, MenuIconButton, separator, useAnchoredMenu } from './Menu';
export type { MenuEntry } from './Menu';

export { Icon, iconForFile, iconNames } from './icons';
export type { IconName, IconProps } from './icons';

// Pre-existing primitives, re-exported so callers have one import site.
export { ConfirmDialog, Modal } from '@/components/Modal';
export { ContextMenuProvider, useContextMenu } from '@/components/ContextMenu';
export { Notifications, notify } from '@/components/Notifications';
export { VirtualList } from '@/components/VirtualList';
