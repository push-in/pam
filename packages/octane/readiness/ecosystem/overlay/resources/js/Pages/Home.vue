<script setup>
import { ref, onMounted } from 'vue';
import { Link } from '@inertiajs/vue3';
defineProps({ message: String, pid: Number });
const broadcast = ref('waiting');
const connected = ref(false);
onMounted(() => {
    window.Echo.connector.pusher.connection.bind('connected', () => { connected.value = true; });
    window.Echo.channel('readiness').listen('.readiness.ping', (e) => { broadcast.value = e.message; });
});
</script>
<template>
    <h1 id="message">{{ message }}</h1>
    <p id="ws">{{ connected ? 'connected' : 'connecting' }}</p>
    <p id="broadcast">{{ broadcast }}</p>
    <Link id="about-link" href="/about">About</Link>
</template>
