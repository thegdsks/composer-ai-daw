import { createContext, useContext } from 'react';
import type { UnsavedGuard } from '../state/unsavedGuard';
import type { ToastQueue } from './toastQueue';

export interface ConfirmOptions {
  title: string;
  message: string;
  confirmLabel: string;
  cancelLabel?: string;
  danger?: boolean;
}

export type Confirm = (options: ConfirmOptions) => Promise<boolean>;

export const ToastsContext = createContext<ToastQueue | null>(null);
export const GuardContext = createContext<UnsavedGuard | null>(null);
export const ConfirmContext = createContext<Confirm | null>(null);

function need<T>(v: T | null, what: string): T {
  if (!v) throw new Error(`${what} is missing`);
  return v;
}

export const useToasts = (): ToastQueue => need(useContext(ToastsContext), 'ToastsContext');
export const useGuard = (): UnsavedGuard => need(useContext(GuardContext), 'GuardContext');
export const useConfirm = (): Confirm => need(useContext(ConfirmContext), 'ConfirmContext');
