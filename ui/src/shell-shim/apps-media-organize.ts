// TODO(phase3): NEED_SDK_CROSS_APP_COMPONENTS — media-organize components

export function OrganizeItemCard(): null {
  return null;
}

export function OrganizeMatchList(): null {
  return null;
}

export function OrganizeActionBar(): null {
  return null;
}

export function useOrganizeSession(): {
  session: null;
  isActive: boolean;
  isLoading: boolean;
} {
  // TODO(phase-6): NEED_SDK_CROSS_APP_COMPONENTS — useOrganizeSession is a stub.
  // The media-organize feature requires host-side components (OrganizeMatchList,
  // OrganizeActionBar) that are not yet extracted to a shared package.
  // When phase-6 extracts these, replace with the real implementation.
  return {
    session: null,
    isActive: false,
    isLoading: false,
  };
}
