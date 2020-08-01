# == Schema Information
#
# Table name: goals
#
#  id            :bigint           not null, primary key
#  completed     :boolean          default(FALSE), not null
#  goal_amt      :integer          not null
#  goal_category :string           not null
#  notes         :string
#  title         :string           not null
#  created_at    :datetime         not null
#  updated_at    :datetime         not null
#  account_id    :integer          not null
#
# Indexes
#
#  index_goals_on_account_id  (account_id)
#
class Goal < ApplicationRecord

  validates_presence_of :goal_amt, :goal_category, :title, :account_id
  validates_inclusion_of :completed, :in => [true, false]
  belongs_to :account,
  foreign_key: :account_id,
  class_name: :Account 

  has_one :user,
  through: :account,
  source: :user 

  def checkGoal
    if self.account.balance > self.goal_amt
      self.completed = true 
    end
  end
end
