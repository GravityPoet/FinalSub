import { lazy, Suspense } from "react";
import { BrowserRouter, Routes, Route } from "react-router-dom";
import Layout from "./components/Layout";
import { I18nProvider } from "./lib/i18n";
import { ThemeProvider } from "./lib/theme";
import "./index.css";

const HomePage = lazy(() => import("./pages/HomePage"));
const TasksPage = lazy(() => import("./pages/TasksPage"));
const ModelsPage = lazy(() => import("./pages/ModelsPage"));
const TranslationPage = lazy(() => import("./pages/TranslationPage"));
const DubbingPage = lazy(() => import("./pages/DubbingPage"));
const VoiceProfilesPage = lazy(() => import("./pages/VoiceProfilesPage"));
const SubtitleMergePage = lazy(() => import("./pages/SubtitleMergePage"));
const SettingsPage = lazy(() => import("./pages/SettingsPage"));
const ProofreadPage = lazy(() => import("./pages/proofread/ProofreadPage"));

function App() {
  return (
    <ThemeProvider>
      <I18nProvider>
        <BrowserRouter>
          <Suspense fallback={<div className="min-h-screen bg-app-bg" />}>
            <Routes>
              <Route element={<Layout />}>
                <Route path="/" element={<HomePage />} />
                <Route path="/tasks" element={<TasksPage />} />
                <Route path="/models" element={<ModelsPage />} />
                <Route path="/translation" element={<TranslationPage />} />
                <Route path="/voices" element={<VoiceProfilesPage />} />
                <Route path="/dubbing" element={<DubbingPage />} />
                <Route path="/proofread" element={<ProofreadPage />} />
                <Route path="/subtitle-merge" element={<SubtitleMergePage />} />
                <Route path="/settings" element={<SettingsPage />} />
              </Route>
            </Routes>
          </Suspense>
        </BrowserRouter>
      </I18nProvider>
    </ThemeProvider>
  );
}

export default App;
