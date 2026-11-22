// The HTTP implementation of the Backend Client: assembles the endpoint
// wrapper groups into the domain-shaped client. This is the transport that
// today serves every domain; F2 switches branches to the IPC adapter per
// module (frontend-architecture-r1 §4/F2).
//
// `BackendClient` (client.ts) is derived from THIS value, so the HTTP shape
// is the contract the IPC adapter must satisfy exactly.
import { ApiError } from '../api';
import {
  aiApi,
  authApi,
  browserApi,
  candidatesApi,
  conversationsApi,
  dashboardApi,
  draftsApi,
  draftGroupsApi,
  followUpsApi,
  importsApi,
  messagesApi,
  positionsApi,
  settingsApi,
} from '../endpoints';

export const httpBackend = {
  candidates: candidatesApi,
  positions: positionsApi,
  conversations: conversationsApi,
  messages: messagesApi,
  imports: importsApi,
  followUps: followUpsApi,
  drafts: draftsApi,
  draftGroups: draftGroupsApi,
  ai: aiApi,
  dashboard: dashboardApi,
  auth: authApi,
  settings: settingsApi,
  browser: browserApi,
} as const;

export { ApiError };

export type HttpBackend = typeof httpBackend;
