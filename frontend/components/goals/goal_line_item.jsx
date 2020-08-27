import React, {useEffect} from 'react'
import { useSelector, useDispatch } from 'react-redux'
import commaFormat from '../../util/number_formatter'
import { deleteGoal } from '../../actions/goal_actions'
import GoalFormContainer from './goal_form_container'
import { openModal } from '../../actions/modal_actions'
//bring in delete function
// bring in update funciton

export default function goal_line_item({goal}) {
  const dispatch = useDispatch()
  let { id, goal_amount, goal_category, title, account_id } = goal;
  const modalOpener = (formType, component, payload) => dispatch(openModal(formType, component, payload))
  const goalDeleter = (goalId) => dispatch(deleteGoal(goalId))

  let account = useSelector((state) => state.entities.accounts[account_id]);
  let goalColorTag = 'red'
  let accountBalance;
  if (!goal_amount) {
    goal_amount = 1.00;
  } else {
    if (goal_amount <= accountBalance) {
      goalColorTag = 'green'
    }
  }
  if (!account) {
    accountBalance = 2.00;
  } else {
    accountBalance = account.balance
  }

  

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
          <span className={goalColorTag}>${commaFormat(accountBalance.toFixed(2).toString())}</span> out
          of <span className="green">${commaFormat(goal_amount.toFixed(2).toString())}</span>
        </span>
        <span className="goal-delete" onClick={() => goalDeleter(id)}>
          Mark as Complete
        </span>
      </div>
    </li>
  );
}
