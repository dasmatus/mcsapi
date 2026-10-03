# mcsapi-components

Component library for mcsapi apps, drawn as native egui widgets and styled by
the shell `Theme` through [`Tokens`](src/tokens.rs).

## Precedence

Components come from three web libraries. When more than one library has the
same component, one source wins:

**React Bits > Aceternity UI > shadcn/ui**

## Licensing

| Library | License | Can it be ported into this repository? |
| --- | --- | --- |
| [shadcn/ui](https://github.com/shadcn-ui/ui) | MIT | Yes, with the MIT notice kept (below). |
| [React Bits](https://github.com/DavidHDev/react-bits) | MIT + Commons Clause | No, not without the author's permission. The clause forbids redistributing the components "alone, in a bundle, or as a ported version". |
| [Aceternity UI](https://ui.aceternity.com/licence) | Custom licence | No, not without permission. Items and derivative works may only ship inside an end product, not be redistributed as source files. |

This crate is a public GPL-3.0 library, which is exactly the redistribution
both restrictive licenses forbid. So this first batch ports **shadcn/ui only**.
Where React Bits or Aceternity UI has the same component, the shadcn version
is a placeholder. The rows marked "pending" below get replaced once licensing
is settled.

shadcn/ui is © 2023 shadcn, used under the MIT License. The components here
are reimplementations in Rust and egui, not copies of the React source.

## Ported (shadcn/ui)

| Component | Rust API | Overlap that wins later |
| --- | --- | --- |
| Accordion | `accordion_item` | |
| Alert | `Alert` | |
| Alert Dialog | `AlertDialog` | |
| Aspect Ratio | `AspectRatio` | |
| Avatar | `Avatar` (initials fallback) | |
| Badge | `Badge` | |
| Breadcrumb | `breadcrumb` | |
| Button | `Button` | |
| Card | `Card` | Pending: React Bits Spotlight/Tilted Card, Aceternity card effects |
| Checkbox | `Checkbox` | |
| Collapsible | `Collapsible` | |
| Dialog | `Dialog` | Pending: Aceternity Animated Modal |
| Empty | `Empty` | |
| Input | `Input` | Pending: Aceternity input / Placeholders and Vanish Input |
| Kbd | `Kbd` | |
| Label | `Label` | |
| Pagination | `Pagination` | |
| Progress | `Progress` | |
| Radio Group | `RadioGroup` | |
| Select | `Select` | |
| Separator | `Separator` | |
| Skeleton | `Skeleton` | |
| Slider | `Slider` | Pending: React Bits Elastic Slider |
| Sonner (toast) | `toast`, `Toaster` | |
| Spinner | `Spinner` | |
| Switch | `Switch` | |
| Table | `Table` | |
| Tabs | `Tabs` | Pending: Aceternity Animated Tabs |
| Textarea | `Textarea` | |
| Toggle | `Toggle` | |
| Toggle Group | `ToggleGroup` (single select) | |
| Tooltip | `tooltip` | Pending: Aceternity Animated Tooltip |
| Typography | `typography::*`, `blockquote` | |

## Not ported yet (shadcn/ui)

Calendar, Date Picker, Carousel, Chart, Combobox, Command, Context Menu, Data
Table, Drawer, Sheet, Dropdown Menu, Menubar, Navigation Menu, Hover Card,
Popover, Input OTP, Resizable, Scroll Area, Sidebar, Button Group, Input Group,
Field, Form, and Item.

Carousel and Sidebar also exist in React Bits or Aceternity UI, so they wait on
the licensing decision.
