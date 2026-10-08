import { afterEach, beforeEach, expect, mock, test } from "bun:test";
import { Window } from "happy-dom";
import {
  createContext,
  type ReactNode,
  useContext,
  useMemo,
  useState,
} from "react";
import { useInfiniteScroll } from "../node_modules/@tokimo/sdk/src/hooks/use-infinite-scroll";
import { Input } from "../node_modules/@tokimo/ui/src/Input";
import type { ListMediaInput } from "../src/api/hooks";
import type { VideoOutput } from "../src/api/types";

const dom = new Window();
Object.assign(globalThis, {
  window: dom,
  document: dom.document,
  navigator: dom.navigator,
  HTMLElement: dom.HTMLElement,
  HTMLInputElement: dom.HTMLInputElement,
  Element: dom.Element,
  Event: dom.Event,
  MouseEvent: dom.MouseEvent,
  IS_REACT_ACT_ENVIRONMENT: true,
});
const { act } = await import("react");
const { createRoot } = await import("react-dom/client");

let width = 900;
const intersections = new Set<(entries: IntersectionObserverEntry[]) => void>();
const resizes = new Set<(entries: ResizeObserverEntry[]) => void>();
class ResizeMock {
  constructor(private callback: (entries: ResizeObserverEntry[]) => void) {}
  observe() {
    resizes.add(this.callback);
  }
  disconnect() {
    resizes.delete(this.callback);
  }
}
class IntersectionMock {
  constructor(
    private callback: (entries: IntersectionObserverEntry[]) => void,
  ) {}
  observe() {
    intersections.add(this.callback);
  }
  disconnect() {
    intersections.delete(this.callback);
  }
}
Object.assign(globalThis, {
  ResizeObserver: ResizeMock,
  IntersectionObserver: IntersectionMock,
  requestAnimationFrame: (fn: FrameRequestCallback) =>
    setTimeout(() => fn(0), 0),
  cancelAnimationFrame: clearTimeout,
});
dom.HTMLElement.prototype.getBoundingClientRect = () => ({
  width,
  height: 400,
  top: 0,
  bottom: 400,
  left: 0,
  right: width,
  x: 0,
  y: 0,
  toJSON: () => ({}),
});

const categories = [
  { id: "movies", type: "movie", name: "Movies" },
  { id: "shows", type: "tv", name: "Shows" },
  { id: "anime", type: "anime", name: "Anime" },
  { id: "online", type: "online_video", name: "Online" },
] as VideoOutput[];
const requests: Array<ListMediaInput & { tv: boolean; enabled?: boolean }> = [];
let failRequests = false;
function useMediaQuery(
  input: ListMediaInput,
  opts: { enabled?: boolean } | undefined,
  tv: boolean,
) {
  requests.push({ ...input, tv, enabled: opts?.enabled });
  const data = useMemo(
    () => ({
      page: input.page ?? 1,
      total: input.search === "absent" ? 0 : 9,
      items:
        input.search === "absent"
          ? []
          : [1, 2, 3].map((n) => ({
              id: `${input.id}-${input.search ?? "all"}-${input.page ?? 1}-${n}`,
              title: `${input.id} ${input.search ?? "all"} ${input.page ?? 1}-${n}`,
              isAdult: false,
              createdAt: "",
              updatedAt: "",
              appId: input.id,
            })),
    }),
    [input.id, input.page, input.search],
  );
  return {
    data: failRequests ? undefined : data,
    isLoading: false,
    isFetching: false,
    isError: failRequests,
    refetch: () => Promise.resolve(),
  };
}
mock.module("../src/api", () => ({
  api: {
    video: {
      list: { useQuery: () => ({ data: categories, isLoading: false }) },
      listGenres: { useQuery: () => ({ data: [] }) },
      listCountries: { useQuery: () => ({ data: [] }) },
      listVideoItems: {
        useQuery: (input: ListMediaInput, opts?: { enabled?: boolean }) =>
          useMediaQuery(input, opts, false),
      },
      listTvShows: {
        useQuery: (input: ListMediaInput, opts?: { enabled?: boolean }) =>
          useMediaQuery(input, opts, true),
      },
    },
  },
  getGenreKey: () => null,
}));
const translate = (key: string, opts?: Record<string, unknown>) =>
  opts?.total === undefined ? key : `${key} ${opts.total}`;
mock.module("react-i18next", () => ({
  useTranslation: () => ({ t: translate }),
}));
mock.module("@tokimo/ui", () => ({
  Input,
  AvatarPicker: () => null,
  cn: (...values: unknown[]) => values.filter(Boolean).join(" "),
  Spin: () => <span>loading</span>,
  Empty: ({ description }: { description: ReactNode }) => (
    <div>{description}</div>
  ),
  AppSetupGuide: () => null,
  PosterCard: ({
    children,
    onClick,
  }: {
    children: ReactNode;
    onClick: () => void;
  }) => (
    <button type="button" onClick={onClick}>
      {children}
    </button>
  ),
}));
mock.module("framer-motion", () => ({
  motion: {
    div: ({
      children,
      layout: _layout,
      transition: _transition,
      ...props
    }: {
      children: ReactNode;
      layout?: boolean;
      transition?: unknown;
    }) => <div {...props}>{children}</div>,
  },
}));
mock.module("@tokimo/sdk", () => ({
  useInfiniteScroll,
  posterThumbUrl: () => null,
  useRuntimeCtx: () => ({}),
  useWindowActions: () => ({}),
  useWindowId: () => "test-window",
  useStandaloneDocumentScroll: () => false,
}));
mock.module("../src/hooks/useSidebarCollapsed", () => ({
  useSidebarCollapsed: () => ({ collapsed: false, onToggleCollapse: () => {} }),
}));
mock.module("../src/hooks/useVideoLibraryProgress", () => ({
  useVideoLibraryProgress: () => ({}),
}));
mock.module("../src/components/ActiveLibraryContext", () => ({
  useSetActiveLibrary: () => {},
}));
mock.module("../src/components/VideoSidebar", () => ({
  default: ({
    onSelect,
    activeId,
  }: {
    onSelect: (id: string) => void;
    activeId: string;
  }) => (
    <aside data-active-id={activeId}>
      {categories.map((c) => (
        <button type="button" key={c.id} onClick={() => onSelect(c.id)}>
          {c.name}
        </button>
      ))}
    </aside>
  ),
}));

interface Nav {
  route: string;
  setRoute: (route: string) => void;
  back: () => void;
}
const NavContext = createContext<Nav | null>(null);
const updateTitle = () => {};
const Detail = () => {
  const nav = useContext(NavContext);
  return (
    <button type="button" onClick={nav?.back}>
      Back
    </button>
  );
};
mock.module("../src/router/useVideoNav", () => ({
  useVideoNav: () => {
    const nav = useContext(NavContext);
    if (!nav) throw new Error("Missing test navigation");
    const [kind, id] = nav.route.slice(1).split("/");
    return {
      params: kind === "library" ? { categoryId: id } : { videoItemId: id },
      replace: nav.setRoute,
      navigate: nav.setRoute,
      updateTitle,
      LazyViewComponent: Detail,
    };
  },
}));
const { default: VideoApp } = await import("../src/components/VideoApp");
function TestWindow({ library = "movies" }: { library?: string }) {
  const [route, setRoute] = useState(`/library/${library}`);
  const [source, setSource] = useState(route);
  const nav = useMemo(
    () => ({
      route,
      setRoute: (next: string) => {
        if (route.startsWith("/library/")) setSource(route);
        setRoute(next);
      },
      back: () => setRoute(source),
    }),
    [route, source],
  );
  return (
    <NavContext value={nav}>
      <VideoApp />
    </NavContext>
  );
}
let container: HTMLDivElement;
let root: ReturnType<typeof createRoot>;
beforeEach(() => {
  width = 900;
  failRequests = false;
  requests.length = 0;
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});
afterEach(async () => {
  await act(() => root.unmount());
  container.remove();
  intersections.clear();
  resizes.clear();
});
async function render(children: ReactNode = <TestWindow />) {
  await act(() => root.render(children));
}
async function click(button: Element | undefined | null) {
  expect(button).toBeTruthy();
  await act(() =>
    button?.dispatchEvent(new MouseEvent("click", { bubbles: true })),
  );
}
function button(text: string, parent: Element = container) {
  return [...parent.querySelectorAll("button")].find(
    (b) => b.textContent === text,
  );
}
function input(parent: Element = container) {
  return parent.querySelector("input") as HTMLInputElement;
}
async function type(value: string, field = input()) {
  expect(field).toBeTruthy();
  await act(() => {
    Object.getOwnPropertyDescriptor(
      HTMLInputElement.prototype,
      "value",
    )?.set?.call(field, value);
    field.dispatchEvent(new Event("input", { bubbles: true }));
  });
}
async function debounce() {
  await act(() => new Promise((resolve) => setTimeout(resolve, 330)));
}
async function loadMore() {
  await act(() => {
    for (const callback of intersections)
      callback([{ isIntersecting: true } as IntersectionObserverEntry]);
  });
}
function scrollOf(card: Element | undefined) {
  return card?.closest("[data-video-scroll], .overflow-y-auto") as HTMLElement;
}

test("detail return keeps loaded pages, source library, scroll and card focus; hidden list pauses pagination", async () => {
  await render();
  await loadMore();
  const card = button("movies all 2-2");
  const scroller = scrollOf(card);
  expect(scroller).toBeTruthy();
  scroller.scrollTop = 620;
  await click(card);
  expect(scroller.isConnected).toBe(true);
  expect(container.querySelector("aside")?.getAttribute("data-active-id")).toBe(
    "movies",
  );
  const page = requests.filter((r) => r.enabled).at(-1)?.page;
  await loadMore();
  expect(requests.filter((r) => r.enabled).at(-1)?.page).toBe(page);
  await click(button("Back"));
  expect(button("movies all 2-2")).toBe(card);
  expect(scroller.scrollTop).toBe(620);
  expect(document.activeElement).toBe(card);
  await click(card);
  await click(button("Back"));
  expect(button("movies all 1-1")).toBeTruthy();
  expect(button("movies all 2-2")).toBe(card);
  expect(
    [...scroller.querySelectorAll("button")].filter((b) =>
      b.textContent?.startsWith("movies all"),
    ),
  ).toHaveLength(6);
});

test("search debounces, starts at page one, combines sort, clears and survives detail return", async () => {
  await render();
  await loadMore();
  const scroller = scrollOf(button("movies all 2-2"));
  scroller.scrollTop = 620;
  await type("Alien");
  expect(requests.filter((r) => r.enabled).at(-1)?.search).toBeUndefined();
  await debounce();
  expect(scroller.scrollTop).toBe(0);
  expect(requests.filter((r) => r.enabled).at(-1)).toMatchObject({
    search: "Alien",
    page: 1,
    id: "movies",
  });
  await click(button("settings.library.sortTitleAsc"));
  expect(requests.filter((r) => r.enabled).at(-1)).toMatchObject({
    search: "Alien",
    sortBy: "title",
    sortDir: "asc",
  });
  await click(button("movies Alien 1-1"));
  await click(button("Back"));
  expect(input().value).toBe("Alien");
  await click(
    container.querySelector('button[aria-label="media.sidebar.clearSearch"]'),
  );
  expect(input().value).toBe("");
  expect(requests.filter((r) => r.enabled).at(-1)?.search).toBeUndefined();
});

test("IME holds the pending search until composition ends", async () => {
  await render();
  await type("old");
  await act(() =>
    input().dispatchEvent(new Event("compositionstart", { bubbles: true })),
  );
  await type("中");
  await debounce();
  expect(requests.filter((r) => r.enabled).at(-1)?.search).toBeUndefined();
  await act(() =>
    input().dispatchEvent(new Event("compositionend", { bubbles: true })),
  );
  await debounce();
  expect(requests.filter((r) => r.enabled).at(-1)?.search).toBe("中");
});

test("windows keep independent browsing state and switching library resets the current list", async () => {
  await render(
    <>
      <section>
        <TestWindow />
      </section>
      <section>
        <TestWindow library="shows" />
      </section>
    </>,
  );
  const sections = container.querySelectorAll("section");
  await type("Matrix", input(sections[0]));
  await debounce();
  expect(input(sections[1]).value).toBe("");
  await click(button("Shows", sections[0]));
  expect(input(sections[0]).value).toBe("");
  expect(button("shows all 1-1", sections[0])).toBeTruthy();
});

test("TV, anime and online libraries send full-library search to the correct endpoint", async () => {
  await render();
  for (const [name, id, tv] of [
    ["Shows", "shows", true],
    ["Anime", "anime", true],
    ["Online", "online", false],
  ] as const) {
    await click(button(name));
    await type("Original title");
    await debounce();
    expect(requests.filter((r) => r.enabled).at(-1)).toMatchObject({
      id,
      tv,
      search: "Original title",
      page: 1,
    });
  }
});

test("search distinguishes no results and request failure with a retry action", async () => {
  await render();
  await type("absent");
  await debounce();
  expect(container.textContent).toContain("media.sidebar.emptySearch");
  failRequests = true;
  await type("failure");
  await debounce();
  expect(container.querySelector('[role="alert"]')?.textContent).toContain(
    "media.sidebar.loadFailed",
  );
  expect(button("media.sidebar.retry")).toBeTruthy();
});

test("return after resize keeps the selected card anchor and ignores zero measurements", async () => {
  await render();
  const card = button("movies all 1-2");
  const wrapper = card?.parentElement as HTMLElement;
  Object.defineProperty(wrapper, "offsetTop", {
    configurable: true,
    value: 600,
  });
  const scroller = scrollOf(card);
  expect(scroller).toBeTruthy();
  scroller.scrollTop = 450;
  await click(card);
  width = 0;
  await act(() => {
    for (const cb of resizes)
      cb([{ contentRect: { width } } as ResizeObserverEntry]);
  });
  width = 500;
  Object.defineProperty(wrapper, "offsetTop", {
    configurable: true,
    value: 1000,
  });
  await click(button("Back"));
  expect(scroller.scrollTop).toBe(850);
  expect(document.activeElement).toBe(card);
});
