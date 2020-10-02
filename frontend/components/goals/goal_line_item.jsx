import React from "react";
import { useSelector, useDispatch } from "react-redux";
import commaFormat from "../../util/number_formatter";
import { deleteGoal } from "../../actions/goal_actions";
import GoalFormContainer from "./goal_form_container";
import { openModal } from "../../actions/modal_actions";
//bring in delete function
// bring in update funciton

export default function goal_line_item({ goal }) {
  const dispatch = useDispatch();
  let { id, goal_amount, goal_category, title, account_id } = goal;
  const modalOpener = (formType, component, payload) =>
    dispatch(openModal(formType, component, payload));
  const goalDeleter = (goalId) => dispatch(deleteGoal(goalId));

  let account = useSelector((state) => state.entities.accounts[account_id]);
  let goalColorTag = "red";
  let accountBalance;
  if (!account) {
    accountBalance = 2.0;
  } else {
    accountBalance = account.balance;
  }

  if (!goal_amount) {
    goal_amount = 1.0;
  } else {
    console.log(goal_amount);
    if (goal_amount <= accountBalance) {
      goalColorTag = "green";
    }
  }

  console.log(goalColorTag);

  return (
    <li className="goal-line-item-container">
      <div className="goal-line-content">
        <img
          src={window.pencil}
          alt="gear"
          onClick={(e) => modalOpener("edit", GoalFormContainer, goal)}
        />
        <div className="goal-line-span">
          <span> {title} </span>
          <span> {goal_category} </span>
        </div>
      </div>
      <div className="goal-right">
        <span className="goal-money-data">
          <span className={goalColorTag}>
            ${commaFormat(accountBalance.toFixed(2).toString())}
          </span>{" "} Saved 
          out of{" "}
          <span className="green">
            ${commaFormat(goal_amount.toFixed(2).toString())}
          </span> Goal
        </span>
        <span className="goal-delete" onClick={() => goalDeleter(id)}>
          Mark as Complete
        </span>
      </div>
    </li>
  );
}
