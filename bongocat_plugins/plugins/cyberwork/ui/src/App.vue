<template>
  <CounterView v-if="view === 'counter'" />
  <HistoryView v-else-if="view === 'history'" />
</template>

<script setup lang="ts">
import CounterView from './CounterView.vue'
import HistoryView from './HistoryView.vue'

const view = new URLSearchParams(window.location.search).get('view') ?? 'counter'
</script>
