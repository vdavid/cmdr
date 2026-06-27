---
title: 'Total Commander for Mac'
date: 2026-06-16
description: 'Why Cmdr is the best Total Commander alternative for macOS'
excerpt:
  "There's no official Total Commander for macOS. The closest thing is [Cmdr](https://getcmdr.com): a dual-pane,
  keyboard-first, very fast orthodox file manager built for Mac, with the same shortcuts and very similar behavior."
---

![Total Commander on Windows: a two-pane file manager with a toolbar and a function-key bar along the bottom](/blog/total-commander-for-macos/totalcmd.webp 'Total Commander on Windows')
![Cmdr on macOS: the same two-pane layout with a clean, modern macOS look](/blog/total-commander-for-macos/cmdr-{theme}.webp 'Cmdr on macOS')
[slider]

I have a [long history](./35-years-of-file-managers) with file managers, and I _loved_ Total Commander while I used
windows. Had I not switched to macOS, I'd still be using it happily.

However, I did switch to macOS, and the **two-pane**, **keyboard-first** file managers that exist on macOS, frankly,
they all **s\*ck**.

If you're interested, you can [download](cmdr:download) Cmdr right now.

## The alternatives

I used [Commander One](https://commander-one.com/) for a while between 2022 and 2026, but as of June, 2026:

- It's extremely slow to access **SMB shares**. This is important for me because I have a home NAS.
- It has a weird artifact when trying to **drag files**: if you don't aim perfectly to the text parts, it starts a
  rectangular file selection, clobbering the existing selection.
- I once pressed delete on a 30 kByte file inside a zip and it **deleted all contents** of my 3 GB zip with no way to
  recover it.
- Generally **feels flimsy**. For example, it happened to me a few times that after deleting some files, they remained
  visible, or disappeared then re-appeared.

[ForkLift](https://binarynights.com/) seems to be a top choice on macOS, and it looks very nice and modern! But when I
tested it in June 2026, it turned out that:

- It doesn't satisfy my **keyboard-first** requirement. I think it was made to be used with a mouse. For example: how do
  you switch volumes (e.g. to a SMB share) in ForkLift?
- It's **slow**: The UI starts lagging heavily even with just 20k files loaded
- It has **no Brief mode**, which is my preferred mode in a file manager. I like to see many files in a folder at once.
- The **left+right sidebars** are unnecessary to me and feel like bloat. I found no way to turn them off.
- Its **UX** is not great. In my short testing, I've managed to get it to a weird "Access denied" state while it had all
  the access it had asked for. I mean, it's fine, bugs do exist, but combined with the other points, it was just the end
  of ForkLift for me.

ForkLift, [Bloom](https://bloomapp.club/), [QSpace Pro](https://qspace.awehunt.com/) and
[Path Finder](https://cocoatech.io/) all fall into the same category for me: their software look nice and modern, but
they have a **mouse-first** feel and, frankly, even if they put in the effort into their designs, the **UX** is just not
there.

I tried a few more too, between 2022 and 2026: [Nimble Commander](https://magnumbytes.com/) had **no Dropbox sync
icons** and silently failed when trying to access a network drive; [Marta](https://marta.sh/) has **no Brief mode** and
was overall basic; [Double Commander](https://github.com/doublecmd/doublecmd), well, while feature-rich, is just
**extremely ugly**, sorry. :(

## How Cmdr compares to Total Commander

|                                                                      | Total Commander                           | Cmdr                                          |
| -------------------------------------------------------------------- | ----------------------------------------- | --------------------------------------------- |
| Platform                                                             | :no: macOS<br>:no: Linux<br>:yes: Windows | :yes: macOS<br>:soon: Linux<br>:soon: Windows |
| Two panes, keyboard-first                                            | :yes: Yes                                 | :yes: Yes                                     |
| Same shortcuts (F3–F8 and the nuanced ones)                          | :yes: Yes                                 | :yes: Yes, plus Finder's                      |
| Brief and Full views, sorting                                        | :yes: Yes                                 | :yes: Yes                                     |
| Built-in file viewer (F3)                                            | :yes: Yes                                 | :yes: Yes                                     |
| Tabs, drag and drop, full clipboard                                  | :yes: Yes                                 | :yes: Yes                                     |
| Command palette                                                      | :warn: Command line and button bar        | :yes: Yes                                     |
| Network drives (SMB)                                                 | :warn: Via plugin                         | :yes: Built-in, and fast                      |
| Android (MTP), Git browser                                           | :warn: Via plugins                        | :yes: Built-in                                |
| Live folder sizes, always (full-disk index)                          | :no: No                                   | :yes: Yes                                     |
| Natural-language and AI search                                       | :no: No                                   | :yes: Yes (early)                             |
| Batch rename, folder sync, FTP/SFTP, archives, plugins, translations | :yes: Yes, mature                         | :soon: Coming                                 |
| Foundation                                                           | Delphi, 20+ years mature                  | Rust, cross-platform core                     |

### The most important similarities

- Both are **very fast** from the ground up
- They share **shortcuts**: not just F3..F8, but all the nuanced ones as well.
- **Two panes**, **keyboard-first** approach, full **clipboard** support, **tabs**, same shortcuts
- **Full mode**, **Brief mode**, **sorting**, shared shortcuts for these.
- Both **work well with the mouse**, incl. in-app and cross-app **drag&drop**.
- They both have a built-in **file viewer** (F3).

### Where Total Commander is better

- Total Commander is 20+ year old software. It's very mature, rock solid, and it has tons of functionality that Cmdr
  doesn't have yet, incl. batch rename, folder sync, FTP(S) + SSH connections, plugins, and i18n (multi-language).

### Where Cmdr is better

- Well, it's **available on macOS**. In addition, Cmdr is **cross-platform** from the ground up: its _first_ target is
  macOS, but it already builds fully for Linux (not yet a supported release), and it's not too hard to add Windows
  support.
- Cmdr meets user expectations on macOS with **modern looks**, a command palette
- Cmdr is written in **Rust**. It's not something visible, but TC is written in Delphi, which is a language that's ~20
  years older than Rust.
- Cmdr has **live drive indexing** which means that it shows the **sizes of all folders**, live, always. It also makes
  searches immediate, and unlocks features like live, quality context we can use for AI-initiated, human-supervised file
  organization.
- A ton of (**optional** and **privacy-first**) **AI features** are coming to Cmdr, with some of them like **natural
  language search** and a built-in MCP server already implemented. The _right_ use of an LLM built into the core of the
  app can make a lot of tasks a lot easier.
- Cmdr also implements **Finder's shortcuts**, so it's easier to use for people who come from Finder and other macOS
  file managers.

All in all, Cmdr is what I wish Total Commander would be in 2026, if it supported macOS.

If this article made you interested and want to try it, [download Cmdr here](cmdr:download).
