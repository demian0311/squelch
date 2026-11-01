import { mount } from 'svelte';
import App from './App.svelte';
import './app.css';
import { load } from './lib/store';

// the app reads its files synchronously from here on
load().then(() => mount(App, { target: document.getElementById('app')! }));
