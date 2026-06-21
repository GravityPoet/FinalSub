import { BrowserRouter, Routes, Route } from "react-router-dom";
import Layout from "./components/Layout";
import HomePage from "./pages/HomePage";
import TasksPage from "./pages/TasksPage";
import ModelsPage from "./pages/ModelsPage";
import TranslationPage from "./pages/TranslationPage";
import SubtitleMergePage from "./pages/SubtitleMergePage";
import SettingsPage from "./pages/SettingsPage";
import ProofreadPage from "./pages/proofread/ProofreadPage";
import { I18nProvider } from "./lib/i18n";
import "./index.css";

function App() {
  return (
    <I18nProvider>
      <BrowserRouter>
        <Routes>
          <Route element={<Layout />}>
            <Route path="/" element={<HomePage />} />
            <Route path="/tasks" element={<TasksPage />} />
            <Route path="/models" element={<ModelsPage />} />
            <Route path="/translation" element={<TranslationPage />} />
            <Route path="/proofread" element={<ProofreadPage />} />
            <Route path="/subtitle-merge" element={<SubtitleMergePage />} />
            <Route path="/settings" element={<SettingsPage />} />
          </Route>
        </Routes>
      </BrowserRouter>
    </I18nProvider>
  );
}

export default App;
