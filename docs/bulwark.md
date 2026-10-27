Programmable Browser

Vision and High-Level Development Plan

1. Overview

Build a fully programmable web browser combining the extensibility of Emacs, the capabilities of a modern web browser, and the expressiveness of Clojure.

The browser should function as both a practical daily driver and a deeply customizable computing environment. Users should be able to inspect, modify, automate, and replace virtually any aspect of its behavior without recompiling or restarting the application.

Core principle: The browser is not an application with scripting added. It is a programmable environment with a web rendering engine integrated into it.

2. Technology Stack

* Rust: Native application infrastructure, operating system integration, security boundaries, and performance-critical components.
* Servo: Embedded web engine providing HTML/CSS rendering, JavaScript execution, DOM interaction, and web platform APIs.
* Clojurust (Corium): Primary application and extension language, providing runtime customization, scripting, asynchronous programming, and interactive development.

The architecture should minimize functionality implemented exclusively in Rust. Wherever practical, user-facing behavior should be implemented in Clojurust using stable, well-defined native APIs.

3. Design Principles

Programmability by default

* Everything accessible through the UI should also be accessible programmatically.
* Commands, keybindings, navigation, layouts, and application behavior should be dynamically customizable.
* Built-in functionality and user extensions should use the same interfaces.

Interactive development

* A persistent REPL enables live inspection and modification.
* Functions, commands, variables, and keybindings should be discoverable and documented.
* Changes should take effect immediately without restarting.

Buffers as universal abstractions

* Everything is represented through buffers: web pages, REPLs, documents, dashboards, inspection tools, and custom interfaces.
* Buffers can be arranged into programmable windows and layouts.
* Major and minor modes define and extend buffer behavior.
* Users can implement entirely new buffer types.

Web content as programmable data

* Expose structured, asynchronous APIs for DOM inspection and manipulation.
* Support navigation hooks, page event subscriptions, content extraction, and custom presentation.
* Enable users to transform websites into alternative interfaces or compose information across multiple sources.

Persistent, inspectable state

* Represent logical application state using immutable, declarative data structures where practical.
* Separate application state from native resources and side effects.
* Support session persistence, introspection, reproducible state transitions, and potentially undo/replay.

Security and isolation

* Maintain strict boundaries between untrusted web content, capability-restricted extensions, and trusted user configuration.
* Browser APIs must enforce permissions at the native boundary.
* Automation and agent capabilities must operate within explicit security constraints.

Practical usability

* The default configuration should provide a familiar, productive browsing experience.
* Advanced programmability must not come at the expense of ordinary browsing functionality.
* The architecture should accommodate Servo’s evolving capabilities and limitations.

4. Proposed Architecture

The system consists of four conceptual layers:

Layer	Responsibility
Servo	Web rendering, JavaScript, DOM, web platform functionality
Rust Core	Native resources, browser engine integration, event dispatch, persistence, security
Clojurust Runtime	Commands, application state, modes, hooks, automation, extension APIs
Programmable UI	Buffers, windows, layouts, navigation, command palette, user-defined interfaces

The Rust core exposes capabilities through a stable API consumed by Clojurust.

The majority of the browser’s visible interface and behavior should be implemented in Clojurust, including its default configuration.

Servo integration should be abstracted sufficiently to avoid unnecessary dependence on internal engine APIs and potentially permit alternative web engines.

5. Key Capabilities

Foundational browser

* Navigation, browsing history, bookmarks, downloads, and persistent profiles.
* Multiple web buffers and flexible window layouts.
* Authentication, cookies, storage, and standard browser permissions.

Programmable environment

* Interactive Clojurust REPL.
* Dynamic command registry and keybinding system.
* User-defined major and minor modes.
* Runtime introspection and documentation.
* Extensible configuration and package loading.

Web automation and transformation

* Structured DOM interaction.
* Custom page transformations and content extraction.
* Event-driven scripts and navigation hooks.
* Programmable dashboards and alternative website interfaces.
* Cross-page workflows and background automation.

Advanced possibilities

* Reactive buffers driven by external data.
* Session snapshots, replay, and state history.
* Agent-driven browsing and automation.
* Programmatically generated browser interfaces.
* Remote control through structured APIs.

These advanced capabilities should inform architectural decisions without becoming prerequisites for the initial implementation.

6. High-Level Development Plan

Stage 1: Architectural Foundation

Establish the integration of Rust, Servo, and Clojurust.

Objectives:

* Create a native application embedding Servo.
* Integrate the Clojurust runtime.
* Establish bidirectional communication between the runtime and browser core.
* Define initial command, event, and native capability APIs.
* Demonstrate interactive control of navigation through a REPL.

Milestone: A minimal web browser whose navigation and basic behavior can be controlled interactively through Clojurust.

Stage 2: Programmable Browser Shell

Implement the foundational Emacs-inspired application model.

Objectives:

* Introduce buffers, windows, and programmable layouts.
* Implement a command registry, keybindings, and command palette.
* Establish major/minor modes and event hooks.
* Move default browser behavior into Clojurust.
* Provide runtime inspection and documentation facilities.

Milestone: A functional browsing environment whose core interface and behavior can be customized dynamically.

Stage 3: Deep Web Integration

Expose web content as a programmable resource.

Objectives:

* Provide asynchronous DOM inspection and manipulation APIs.
* Implement navigation and page lifecycle hooks.
* Support content extraction, JavaScript evaluation, and page transformations.
* Introduce custom buffer types and programmable content views.
* Establish security and capability boundaries for scripts.

Milestone: Users can implement sophisticated browser extensions and alternative web interfaces entirely in Clojurust.

Stage 4: Daily-Driver Viability

Develop the functionality and reliability required for ordinary browsing.

Objectives:

* Implement persistent profiles, history, bookmarks, and session restoration.
* Support downloads, authentication, permissions, and common browser workflows.
* Improve usability, stability, recovery, and performance.
* Evaluate compatibility with major websites and identify Servo limitations.
* Establish packaging and distribution.

Milestone: A practical browser suitable for sustained everyday use, subject to upstream engine compatibility.

Stage 5: Advanced Programmability

Extend the browser toward a general-purpose programmable environment.

Potential directions:

* Reactive application state and programmable data views.
* Browser state snapshots, replay, and automation.
* Cross-site data aggregation and custom workflows.
* Persistent background tasks.
* Agent integration and externally controlled browsing.
* Package management and a community extension ecosystem.

Milestone: A browser that serves as a platform for building new applications and interaction models, rather than merely displaying existing websites.

7. Key Technical Risks

Risk	Proposed Approach
Servo compatibility and maturity	Validate embedding early; isolate engine integration behind well-defined interfaces
Clojurust runtime maturity	Use the browser as a focused integration test while keeping runtime dependencies explicit
Async execution and thread ownership	Define clear ownership, scheduling, and message-passing boundaries
DOM API stability and safety	Prefer asynchronous, handle-based APIs with explicit lifetimes and invalidation
Extension security	Enforce capability boundaries in Rust rather than relying on script conventions
Performance and responsiveness	Separate rendering, UI interaction, and extension execution appropriately
Project scope	Prioritize a small, programmable vertical slice before implementing comprehensive browser features

8. Initial Proof of Concept

The first implementation should validate the project’s central architectural hypothesis:

Can a useful browser shell be implemented primarily in Clojurust, with Rust and Servo providing native mechanisms rather than application policy?

A successful proof of concept should demonstrate:

1. A native window rendering a web page through Servo.
2. An embedded Clojurust runtime with an interactive REPL.
3. Navigation initiated programmatically from Clojurust.
4. Browser events delivered to Clojurust callbacks.
5. Dynamically defined commands and keybindings.
6. At least one meaningful browser behavior implemented entirely in Clojurust.

This proof of concept deliberately excludes most daily-driver functionality.

Its purpose is to validate the architecture before committing to a larger implementation.

9. Next Steps

The immediate next activity is to decompose the initial development plan into concrete, independently verifiable phases.

That decomposition should establish:

* Architecture and API decisions required before implementation.
* Dependencies between Servo integration, Clojurust runtime capabilities, and the browser shell.
* Specific deliverables and acceptance criteria for each phase.
* Technical experiments needed to resolve high-risk assumptions.
* A minimal vertical slice demonstrating the end-to-end architecture.

Development should proceed incrementally, with each phase producing a working system that exercises the architectural principles established here.

10. Long-Term Vision

The ultimate goal is an environment where virtually every aspect of browsing can be inspected, scripted, customized, or replaced.

Ordinary users receive a capable web browser. Power users gain the ability to reshape their browsing experience. Developers gain a programmable platform for constructing entirely new interfaces and workflows.

The browser should be as malleable as Emacs, as capable as a modern web browser, and as expressive as Clojure.
