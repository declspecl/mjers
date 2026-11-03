import App, { PracticePage } from "./app";
import { Room } from "./room";
import { parseSearch, stringifySearch, validatePracticeSearch, validateRoomSearch } from "./routing/search";
import { createRootRoute, createRoute, createRouter, Link, redirect } from "@tanstack/react-router";

const rootRoute = createRootRoute({
    component: App,
    notFoundComponent: () => (
        <section className="room-screen">
            <div>
                <h1>Page not found</h1>
                <Link to="/" search={{}}>
                    Back to rooms
                </Link>
            </div>
        </section>
    ),
});

const entryRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: "/",
    validateSearch: validateRoomSearch,
    beforeLoad: ({ search }) => {
        if (search.room && search.seed !== undefined) {
            throw redirect({ to: "/", search: { room: search.room }, replace: true });
        }

        if (!search.room && search.seed !== undefined) {
            throw redirect({ to: "/practice", search: { seed: search.seed }, replace: true });
        }
    },
    component: Room,
});

const practiceRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: "/practice",
    validateSearch: validatePracticeSearch,
    component: PracticePage,
});

export const router = createRouter({
    routeTree: rootRoute.addChildren([entryRoute, practiceRoute]),
    parseSearch,
    stringifySearch,
});

declare module "@tanstack/react-router" {
    interface Register {
        router: typeof router;
    }
}
