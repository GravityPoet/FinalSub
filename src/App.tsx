import { BrowserRouter, Routes, Route } from "react-router-dom";
import Layout from "./components/Layout";
import HomePage from "./pages/HomePage";
import TasksPage from "./pages/TasksPage";
import ModelsPage from "./pages/ModelsPage";
import PlaceholderPage from "./pages/PlaceholderPage";
import "./index.css";
import { Edit3, Film, Languages, Settings } from "lucide-react";

function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route element={<Layout />}>
          <Route path="/" element={<HomePage />} />
          <Route path="/tasks" element={<TasksPage />} />
          <Route path="/models" element={<ModelsPage />} />
          <Route path="/translation" element={<PlaceholderPage title="翻译管理" icon={Languages} />} />
          <Route path="/proofread" element={<PlaceholderPage title="字幕校对" icon={Edit3} />} />
          <Route path="/subtitle-merge" element={<PlaceholderPage title="视频合字幕" icon={Film} />} />
          <Route path="/settings" element={<PlaceholderPage title="设置" icon={Settings} />} />
        </Route>
      </Routes>
    </BrowserRouter>
  );
}

export default App;
