import React from "react";
import { Route, Switch} from "react-router-dom";
import LoginFormContainer from "./session/login_form_container";
import SignupFormContainer from "./session/sign_up_form_container";
import { AuthRoute, ProtectedRoute } from "../util/route_util";
// import DashHeaderContainer from './dash_top/dash_header_container'
import OverviewContainer from './overview/overview_container'
import SplashPage from './splash/splash_page'
import DashTop from "./dash_top/dash_top";
import Modal from './modal'
const App = ({store}) => {

  return (
    <>
      <Modal />
      
      <AuthRoute exact path="/" component={SplashPage} />
      <ProtectedRoute path={["/overview", "/transactions", '/goals', '/bills']} component={DashTop} />
      
      <Switch>
        <AuthRoute exact path="/login" component={LoginFormContainer} />
        <AuthRoute exact path="/signup" component={SignupFormContainer} />
        <ProtectedRoute exact path={["/overview", "/overview/aa"]} component={OverviewContainer} />


        {/* <ProtectedRoute path="/transactions" component={Overview} />
        <ProtectedRoute path="/goals" component={Overview} />
        <ProtectedRoute path="/bills" component={Overview} /> */}
        
      </Switch>
    </>
  );
};

export default App;
