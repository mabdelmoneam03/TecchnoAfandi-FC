import { load } from '@tauri-apps/plugin-store';

let storeInstance = null;

async function getStore() {
    if (!storeInstance) {
        storeInstance = await load('settings.json', { autoSave: false });
    }
    return storeInstance;
}

export async function setGamePath(path) {
    try {
        const store = await getStore();
        await store.set('gamePath', path);
        await store.save();
    } catch (e) {
        console.error("Failed to set game path", e);
        throw e;
    }
}

export async function getGamePath() {
    try {
        const store = await getStore();
        return await store.get('gamePath');
    } catch (e) {
        console.error("Failed to get game path", e);
        return null;
    }
}

export async function clearGamePath() {
    try {
        const store = await getStore();
        await store.delete('gamePath');
        await store.save();
    } catch (e) {
        console.error("Failed to clear game path", e);
    }
}
