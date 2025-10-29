import { useState } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { LayoutDashboard, Database, Search, HardDrive, Key, Menu, X, Settings, Users } from 'lucide-react';
import Dashboard from './components/Dashboard';
import DataSubmit from './components/DataSubmit';
import DataQuery from './components/DataQuery';
import BlobManager from './components/BlobManager';
import { KeyPairManager } from './components/KeyPairManager';
import { SettingsModal } from './components/Settings';
import PeerConnection from './components/PeerConnection';

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      refetchOnWindowFocus: false,
      retry: 1,
    },
  },
});

type Page = 'dashboard' | 'submit' | 'query' | 'blobs' | 'keypair' | 'peers';

function App() {
  const [currentPage, setCurrentPage] = useState<Page>('dashboard');
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [settingsOpen, setSettingsOpen] = useState(false);

  const navigation = [
    { id: 'dashboard' as Page, name: 'Dashboard', icon: LayoutDashboard },
    { id: 'keypair' as Page, name: 'KeyPair', icon: Key },
    { id: 'submit' as Page, name: 'Store Data', icon: Database },
    { id: 'query' as Page, name: 'Query Data', icon: Search },
    { id: 'blobs' as Page, name: 'Blob Storage', icon: HardDrive },
    { id: 'peers' as Page, name: 'Connect Peer', icon: Users },
  ];

  const renderPage = () => {
    switch (currentPage) {
      case 'dashboard':
        return <Dashboard />;
      case 'keypair':
        return <KeyPairManager />;
      case 'submit':
        return <DataSubmit />;
      case 'query':
        return <DataQuery />;
      case 'blobs':
        return <BlobManager />;
      case 'peers':
        return <PeerConnection />;
      default:
        return <Dashboard />;
    }
  };

  return (
    <QueryClientProvider client={queryClient}>
      <div className="min-h-screen bg-gray-100">
        {/* Sidebar */}
        <aside
          className={`fixed inset-y-0 left-0 z-50 w-64 bg-white shadow-lg transform transition-transform duration-200 ease-in-out ${
            sidebarOpen ? 'translate-x-0' : '-translate-x-full'
          }`}
        >
          <div className="flex items-center justify-between p-4 border-b">
            <h1 className="text-xl font-bold text-gray-900">CyberFly</h1>
            <button
              onClick={() => setSidebarOpen(false)}
              className="lg:hidden p-2 rounded-md hover:bg-gray-100"
            >
              <X className="w-5 h-5" />
            </button>
          </div>

          <nav className="p-4 space-y-2">
            {navigation.map((item) => {
              const Icon = item.icon;
              const isActive = currentPage === item.id;
              return (
                <button
                  key={item.id}
                  onClick={() => {
                    setCurrentPage(item.id);
                    // Only close sidebar on mobile
                    if (window.innerWidth < 1024) {
                      setSidebarOpen(false);
                    }
                  }}
                  className={`w-full flex items-center gap-3 px-4 py-3 rounded-lg transition ${
                    isActive
                      ? 'bg-blue-600 text-white'
                      : 'text-gray-700 hover:bg-gray-100'
                  }`}
                >
                  <Icon className="w-5 h-5" />
                  <span className="font-medium">{item.name}</span>
                </button>
              );
            })}
          </nav>

          <div className="absolute bottom-0 left-0 right-0 p-4 border-t bg-gray-50">
            <div className="text-xs text-gray-600">
              <p className="font-medium mb-1">Rust Backend</p>
              <p>Version 0.1.0</p>
              <p className="mt-2">Iroh Network + Sled DB</p>
            </div>
          </div>
        </aside>

        {/* Mobile menu button */}
        <button
          onClick={() => setSidebarOpen(true)}
          className={`lg:hidden fixed top-4 left-4 z-40 p-2 bg-white rounded-md shadow-lg ${
            sidebarOpen ? 'hidden' : 'block'
          }`}
        >
          <Menu className="w-6 h-6" />
        </button>

        {/* Settings button */}
        <button
          onClick={() => setSettingsOpen(true)}
          className="fixed top-4 right-4 z-40 p-2 bg-white rounded-md shadow-lg hover:bg-gray-50 transition"
          title="Settings"
        >
          <Settings className="w-5 h-5 text-gray-700" />
        </button>

        {/* Main content */}
        <main
          className={`transition-all duration-200 ${
            sidebarOpen ? 'lg:ml-64' : 'ml-0'
          }`}
        >
          {renderPage()}
        </main>

        {/* Settings Modal */}
        <SettingsModal isOpen={settingsOpen} onClose={() => setSettingsOpen(false)} />

        {/* Overlay for mobile */}
        {sidebarOpen && (
          <div
            className="lg:hidden fixed inset-0 bg-black bg-opacity-50 z-40"
            onClick={() => setSidebarOpen(false)}
          />
        )}
      </div>
    </QueryClientProvider>
  );
}

export default App;
