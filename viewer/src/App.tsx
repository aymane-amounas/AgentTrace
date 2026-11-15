import { NavLink, Outlet } from "react-router-dom";

function NavItem({ to, label, icon, end }: { to: string; label: string; icon: string; end?: boolean }) {
  return (
    <NavLink to={to} end={end} className={({ isActive }) => `nav-link${isActive ? " active" : ""}`}>
      <span aria-hidden>{icon}</span>
      {label}
    </NavLink>
  );
}

export default function App() {
  return (
    <div className="app">
      <aside className="sidebar">
        <div className="brand">
          <span className="logo">
            <span />
          </span>
          AgentTrace
        </div>
        <NavItem to="/" label="Overview" icon="◧" end />
        <NavItem to="/traces" label="Traces" icon="≋" />
        <NavItem to="/settings" label="Privacy" icon="⛊" />
        <div className="spacer" />
        <div className="foot">
          Local observability for AI agents.
          <br />
          Loopback-only · redaction on.
        </div>
      </aside>
      <main className="main">
        <Outlet />
      </main>
    </div>
  );
}
