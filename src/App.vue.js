/// <reference types="G:/Tools/RustProxyHub/.toolchain/node_modules/@vue/language-core/types/template-helpers.d.ts" />
/// <reference types="G:/Tools/RustProxyHub/.toolchain/node_modules/@vue/language-core/types/props-fallback.d.ts" />
import { storeToRefs } from 'pinia';
import { onBeforeUnmount, onMounted } from 'vue';
import DetailsDrawer from '@/components/dashboard/DetailsDrawer.vue';
import HubHeader from '@/components/dashboard/HubHeader.vue';
import LoginStudio from '@/components/dashboard/LoginStudio.vue';
import ProviderGrid from '@/components/dashboard/ProviderGrid.vue';
import QwenAccountsPanel from '@/components/dashboard/QwenAccountsPanel.vue';
import WorkbenchPanel from '@/components/dashboard/WorkbenchPanel.vue';
const store = useStore();
const { overview, error, filteredProviders } = storeToRefs(store);
onMounted(() => {
    void store.initApp();
});
onBeforeUnmount(() => {
    store.disposeApp();
});
const __VLS_ctx = {
    ...{},
    ...{},
};
let __VLS_components;
let __VLS_intrinsics;
let __VLS_directives;
__VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({
    ...{ class: "control-room" },
});
/** @type {__VLS_StyleScopedClasses['control-room']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.div)({
    ...{ class: "backdrop-haze haze-a" },
});
/** @type {__VLS_StyleScopedClasses['backdrop-haze']} */ ;
/** @type {__VLS_StyleScopedClasses['haze-a']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.div)({
    ...{ class: "backdrop-haze haze-b" },
});
/** @type {__VLS_StyleScopedClasses['backdrop-haze']} */ ;
/** @type {__VLS_StyleScopedClasses['haze-b']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.div)({
    ...{ class: "backdrop-grid" },
});
/** @type {__VLS_StyleScopedClasses['backdrop-grid']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.div)({
    ...{ class: "scanline" },
});
/** @type {__VLS_StyleScopedClasses['scanline']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.div)({
    ...{ class: "fracture fracture-a" },
});
/** @type {__VLS_StyleScopedClasses['fracture']} */ ;
/** @type {__VLS_StyleScopedClasses['fracture-a']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.div)({
    ...{ class: "fracture fracture-b" },
});
/** @type {__VLS_StyleScopedClasses['fracture']} */ ;
/** @type {__VLS_StyleScopedClasses['fracture-b']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.main, __VLS_intrinsics.main)({
    ...{ class: "frame" },
});
/** @type {__VLS_StyleScopedClasses['frame']} */ ;
const __VLS_0 = HubHeader;
// @ts-ignore
const __VLS_1 = __VLS_asFunctionalComponent1(__VLS_0, new __VLS_0({
    overview: (__VLS_ctx.overview),
}));
const __VLS_2 = __VLS_1({
    overview: (__VLS_ctx.overview),
}, ...__VLS_functionalComponentArgsRest(__VLS_1));
if (__VLS_ctx.error) {
    __VLS_asFunctionalElement1(__VLS_intrinsics.p, __VLS_intrinsics.p)({
        ...{ class: "error-banner" },
    });
    /** @type {__VLS_StyleScopedClasses['error-banner']} */ ;
    (__VLS_ctx.error);
}
__VLS_asFunctionalElement1(__VLS_intrinsics.section, __VLS_intrinsics.section)({
    ...{ class: "board" },
});
/** @type {__VLS_StyleScopedClasses['board']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({
    ...{ class: "lane services-lane" },
});
/** @type {__VLS_StyleScopedClasses['lane']} */ ;
/** @type {__VLS_StyleScopedClasses['services-lane']} */ ;
const __VLS_5 = ProviderGrid;
// @ts-ignore
const __VLS_6 = __VLS_asFunctionalComponent1(__VLS_5, new __VLS_5({
    providers: (__VLS_ctx.filteredProviders),
}));
const __VLS_7 = __VLS_6({
    providers: (__VLS_ctx.filteredProviders),
}, ...__VLS_functionalComponentArgsRest(__VLS_6));
const __VLS_10 = QwenAccountsPanel;
// @ts-ignore
const __VLS_11 = __VLS_asFunctionalComponent1(__VLS_10, new __VLS_10({}));
const __VLS_12 = __VLS_11({}, ...__VLS_functionalComponentArgsRest(__VLS_11));
__VLS_asFunctionalElement1(__VLS_intrinsics.aside, __VLS_intrinsics.aside)({
    ...{ class: "lane side-lane" },
});
/** @type {__VLS_StyleScopedClasses['lane']} */ ;
/** @type {__VLS_StyleScopedClasses['side-lane']} */ ;
const __VLS_15 = WorkbenchPanel;
// @ts-ignore
const __VLS_16 = __VLS_asFunctionalComponent1(__VLS_15, new __VLS_15({}));
const __VLS_17 = __VLS_16({}, ...__VLS_functionalComponentArgsRest(__VLS_16));
const __VLS_20 = LoginStudio;
// @ts-ignore
const __VLS_21 = __VLS_asFunctionalComponent1(__VLS_20, new __VLS_20({
    providers: (__VLS_ctx.filteredProviders),
}));
const __VLS_22 = __VLS_21({
    providers: (__VLS_ctx.filteredProviders),
}, ...__VLS_functionalComponentArgsRest(__VLS_21));
const __VLS_25 = DetailsDrawer;
// @ts-ignore
const __VLS_26 = __VLS_asFunctionalComponent1(__VLS_25, new __VLS_25({}));
const __VLS_27 = __VLS_26({}, ...__VLS_functionalComponentArgsRest(__VLS_26));
// @ts-ignore
[overview, error, error, filteredProviders, filteredProviders,];
const __VLS_export = (await import('vue')).defineComponent({});
export default {};
