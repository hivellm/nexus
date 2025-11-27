import { createRouter, createWebHashHistory } from 'vue-router';
import type { RouteRecordRaw } from 'vue-router';

const routes: RouteRecordRaw[] = [
  {
    path: '/',
    name: 'Dashboard',
    component: () => import('@/views/DashboardView.vue'),
  },
  {
    path: '/query',
    name: 'Query',
    component: () => import('@/views/QueryView.vue'),
  },
  {
    path: '/graph',
    name: 'Graph',
    component: () => import('@/views/GraphView.vue'),
  },
  {
    path: '/schema',
    name: 'Schema',
    component: () => import('@/views/SchemaView.vue'),
  },
  {
    path: '/data',
    name: 'Data',
    component: () => import('@/views/DataView.vue'),
  },
  {
    path: '/indexes',
    name: 'Indexes',
    component: () => import('@/views/IndexesView.vue'),
  },
  {
    path: '/logs',
    name: 'Logs',
    component: () => import('@/views/LogsView.vue'),
  },
  {
    path: '/config',
    name: 'Config',
    component: () => import('@/views/ConfigView.vue'),
  },
];

const router = createRouter({
  history: createWebHashHistory(),
  routes,
});

export default router;
