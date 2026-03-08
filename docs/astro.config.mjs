import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";

export default defineConfig({
  integrations: [
    starlight({
      title: "Moneypenny",
      description:
        "The enterprise-grade dynamic intelligence layer for agents",
      social: [
        {
          icon: "github",
          label: "GitHub",
          href: "https://github.com/jacobprall/moneypenny",
        },
      ],
      editLink: {
        baseUrl:
          "https://github.com/jacobprall/moneypenny/edit/main/docs/",
      },
      sidebar: [
        {
          label: "Getting Started",
          items: [
            { label: "Introduction", slug: "introduction" },
            { label: "Quickstart", slug: "quickstart" },
          ],
        },
        {
          label: "Core Concepts",
          items: [
            { label: "Agents", slug: "concepts/agents" },
            { label: "Memory & Facts", slug: "concepts/memory-and-facts" },
            { label: "Knowledge Base", slug: "concepts/knowledge" },
            { label: "Search", slug: "concepts/search" },
            { label: "Policy Engine", slug: "concepts/policy-engine" },
            { label: "Skills & Tools", slug: "concepts/skills-and-tools" },
            { label: "Scheduled Jobs", slug: "concepts/scheduled-jobs" },
            { label: "Sync", slug: "concepts/sync" },
          ],
        },
        {
          label: "Guides",
          items: [
            { label: "Ingestion", slug: "guides/ingestion" },
            { label: "Governance", slug: "guides/governance" },
            { label: "Multi-Agent", slug: "guides/multi-agent" },
            {
              label: "Gateway & Channels",
              slug: "guides/gateway-and-channels",
            },
            {
              label: "Sidecar & Integrations",
              slug: "guides/sidecar-and-integrations",
            },
          ],
        },
        {
          label: "CLI Reference",
          items: [{ label: "Commands", slug: "cli/reference" }],
        },
        {
          label: "Architecture",
          items: [
            { label: "Overview", slug: "architecture/overview" },
            {
              label: "Canonical Operations",
              slug: "architecture/canonical-operations",
            },
          ],
        },
      ],
      customCss: [],
    }),
  ],
});
