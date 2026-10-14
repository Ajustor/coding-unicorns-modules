<template>
  <div :class="{ active: isOn }" @click="toggle">
    <p v-if="items.length">{{ items.length }} items for {{ user?.name }}</p>
    <ItemRow v-for="item in items" :key="item.id" :item="item" />
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from "vue";

const props = defineProps<{ start: number }>();
const isOn = ref(false);
const items = computed(() => [{ id: props.start }]);
function toggle(): void {
  isOn.value = !isOn.value;
}
</script>

<style scoped>
div.active > p {
  color: #42b883;
  margin: 0 auto;
}
</style>
