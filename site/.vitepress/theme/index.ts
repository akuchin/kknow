import { h } from "vue";
import DefaultTheme from "vitepress/theme";
import type { Theme } from "vitepress";
import Mermaid from "./Mermaid.vue";
import Screenshot from "./Screenshot.vue";
import "./custom.css";

export default {
  extends: DefaultTheme,
  // The app screenshot goes between the hero and the feature cards.
  Layout: () => h(DefaultTheme.Layout, null, { "home-hero-after": () => h(Screenshot) }),
  enhanceApp({ app }) {
    app.component("Mermaid", Mermaid);
  },
} satisfies Theme;
