<script setup lang="ts">
import { withBase } from "vitepress";
</script>

<template>
  <div class="screenshot">
    <img
      :src="withBase('/screenshot.png')"
      alt="The kknow window: the vault tree on the left, a note in live preview on the right"
    />
  </div>
</template>
