# == Schema Information
#
# Table name: goals
#
#  id            :bigint           not null, primary key
#  goal_amount   :float            not null
#  goal_category :string           not null
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
  validates_presence_of :goal_amount, :goal_category, :title, :account_id 
  validates :goal_category, inclusion: { in: %w(Retirement Wedding College Travel Emergency Purchase General Other) }

  belongs_to :account
end
