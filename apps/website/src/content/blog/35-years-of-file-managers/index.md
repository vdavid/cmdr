---
title: '35 years of file managers'
date: 2026-06-15
description: 'I was a big Norton and Total Commander fan, but found no great macOS alternative, so I created Cmdr.'
excerpt: 'And how I ended up building my own.'
---

## Norton Commander

When I got my first PC at age 6, I cried. It was **1991**, and I'd really wanted a
[Commodore 64](https://en.wikipedia.org/wiki/Commodore_64#/media/File:Commodore-64-Computer-FL.jpg), because I knew it
had a lot of games. And instead, I got this crappy PC, with an
[Intel 286 CPU](https://en.wikipedia.org/wiki/Intel_80286), 40 MB of HDD, and a monochrome screen.

I had three pieces of software on this PC:

1. [This Tetris game](https://dosgames.com/game/tetris/)
2. [QuickBASIC](https://dos.zone/qbasic-1991/) (Type `PRINT "Well Hello world!"` under "Untitled" and press F5!)
3. [Norton Commander](https://dos.zone/norton-commander/) (2.0 or so)

Did you count? That's **1** game. I felt really disappointed.

Got really bored with **Tetris** after a week. **Norton Commander** was a lot more interesting! Creating files with
funny 8+3 char names like "DUCKCOCK.EXE", deleting my OS and then not being able to boot the computer, creating dirs,
looking into binary files — now these were a lot more interesting!

Then eventually I got bored with that too, and started playing with **QuickBASIC**, which led me to where I am today.
But I'll never forget Norton Commander which, I feel, was kinda one of the _places_ where I grew up.

## Total Commander

After spending years in Windows 3.1, Windows 95 appeared, and with it, file names that could be more than 8+3 chars, and
soon enough, seeing all the `PROGRA~1` dirs made Norton Commander (NC) less fun. That was the time when **Windows
Commander** (WC) got huge. It [looked](https://www.youtube.com/watch?v=V5ciEKKdQOk) pretty much like what Total
Commander (TC) looks like today.

It was weird at first, and I remember using NC in parallel for quite some time, but eventually I fully converted to WC.
So much so that around 1997, I hand-made a **256-color icon set** for it because it only had 16-color icons at the time.
[Christian Ghisler](https://www.ghisler.ch/wiki/index.php/Christian_Ghisler) included it in the app and sent me a
license to WC, which I hold to this day! ❤️

In 2002, Microsoft expressed concerns about the name, so the author [renamed it](https://www.ghisler.com/name.htm) to
**[Total Commander](https://www.ghisler.com/)**.

## Commander One

For two decades, Total Commander was the #1 software I installed on a new computer. But then, in 2021, I switched to
**macOS**. On macOS, my main gripe was missing TC. I tried
[Total Commander under Wine](https://www.ghisler.ch/wiki/index.php/Total_Commander_under_Wine),
[Double Commander](https://github.com/doublecmd/doublecmd), [muCommander](https://www.mucommander.com/),
[Nimble Commander](https://magnumbytes.com/), and a few others, and eventually got settled with
**[Commander One](https://commander-one.com/)**, which I found to be the least bad of the options. But it's still rather
bad, unfortunately.

Now, if you grew up on Windows Explorer, then Finder is probably pretty OK for you. You probably use your mouse and
multiple windows to manage your files. But _I_ want a **single window** with **two panes** and use my **keyboard**. I
guess a lot of people who prefer the keyboard use a terminal, even [mc](https://midnight-commander.org/). Not me. I like
a GUI. I think it uses my screen better, looks better, and maybe I also just got used to it over the years.

## Cmdr

That said, I was not satisfied with Commander One, at all. Some weird bugs always bothered me, and they didn't get fixed
over the years, which signals to me that its creators don't care anymore. When I bought a home server and the connection
via SMB was inexplicably slow from Commander One, it was the last straw and I started creating Cmdr.

Cmdr is:

- **Keyboard-first** with **two panes**.
- As **fast** as Total Commander.
- A **macOS** app.

Plus I think it looks a lot more modern and nicer than TC, but that's more subjective.

About the SMB angle: I went somewhat overboard with it, and wrote my own pure-Rust lib
[smb2](https://github.com/vdavid/smb2) to ensure it's not as slow as Commander One. It went so well that the lib is
[several times faster](https://www.veszelovszki.com/a/smb2/) than macOS's built-in SMB client.

Cmdr is not my ideal file manager **yet** but I'm working hard every day to make it that.

If you want to join me, Cmdr is now in **open beta**, you can [download](cmdr:download) it for free to manage your
personal files (for work files, you'll need to convince your manager to [buy it for you](https://getcmdr.com/pricing)),
and I'd love it if you could give me feedback on it to help me make it the truly _best_ TC-like file manager for macOS!
