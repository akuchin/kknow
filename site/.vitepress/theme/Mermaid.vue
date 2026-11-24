<script setup lang="ts">
import { onMounted, ref, watch } from "vue";
import { useData } from "vitepress";

const props = defineProps<{ code: string }>();
const { isDark } = useData();
const svg = ref("");
let count = 0;

async function draw() {
  const { default: mermaid } = await import("mermaid");
  mermaid.initialize({
    startOnLoad: false,
    securityLevel: "strict",
    theme: isDark.value ? "dark" : "neutral",
  });
  const id = `mermaid-${Math.random().toString(36).slice(2)}-${count++}`;
  const result = await mermaid.render(id, decodeURIComponent(props.code));
  svg.value = result.svg;
}

onMounted(draw);
watch(isDark, draw);
</script>

<template>
  <div class="mermaid-diagram" v-html="svg" />
</template>

<style scoped>
.mermaid-diagram {
  display: flex;
  justify-content: center;
  margin: 16px 0;
  min-height: 48px;
}

.mermaid-diagram :deep(foreignObject p) {
  margin: 0;
  line-height: 1.5;
}
</style>
