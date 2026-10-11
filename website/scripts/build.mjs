import { cpSync, mkdirSync } from 'node:fs';

const output = new URL('../dist/', import.meta.url);
mkdirSync(output, { recursive: true });
cpSync(new URL('../public/', import.meta.url), output, { recursive: true });
console.log('Built Hazlo Mexa website in website/dist.');
