import React, { useEffect } from "react";
import { useDispatch } from "react-redux";
import { Route } from "react-router-dom";
import AccountsIndex from "../accounts/accounts_index";
import TransactionIndex from "../transactions/transaction_index";
import GoalIndex from "../goals/goal_index";
import { requestAccounts } from "../../actions/account_actions";
import { requestTransactions } from "../../actions/transaction_actions";
import { requestGoals } from "../../actions/goal_actions";
import BillsIndex from "../bills/bills_index";
import Chart from "../accounts/chart";
import { requestBills } from "../../actions/bill_actions";
import { requestBusinessNews } from "../../actions/news_actions";
export default function overview() {
  // dispatch and retrieve accounts
  const dispatch = useDispatch();
  const getTransactions = () => dispatch(requestTransactions());
  const getAccounts = () => dispatch(requestAccounts());
  const getGoals = () => dispatch(requestGoals());
  const getBills = () => dispatch(requestBills());
  const getNews = () => dispatch(requestBusinessNews());
  useEffect(() => {
    getNews();
    getAccounts();
    getGoals();
    getTransactions();
    getBills();
  }, []);

  return (
    <main className="overview-page">
      <Route exact path="/overview">
        <AccountsIndex />
      </Route>
      <Route exact path="/overview/transactions">
        <TransactionIndex />
      </Route>
      <Route exact path="/overview/goals">
        <GoalIndex />
      </Route>
      <Route exact path="/overview/bills">
        <BillsIndex />
      </Route>
    </main>
  );
}
